use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, io::{Cursor, Read, Write}, path::{Path, PathBuf}, sync::{Mutex, atomic::{AtomicU64, Ordering}}, time::{SystemTime, UNIX_EPOCH}};
use serde_json::{json, Value};
use std::hash::{Hash, Hasher};

pub const PARTS: [&str; 6] = ["headOpen", "headClosed", "body", "tail", "leftArm", "rightArm"];
const MAX_FILE: usize = 20 * 1024 * 1024;
const MAX_IMAGE: usize = 10 * 1024 * 1024;
static NEXT_WRITE: AtomicU64 = AtomicU64::new(0);
static MUTATIONS: Mutex<()> = Mutex::new(());

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Crop { pub x: f64, pub y: f64, pub w: f64, pub h: f64 }
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all="camelCase", deny_unknown_fields)]
pub struct Rig { pub head_scale:f64, pub body_scale:f64, pub head_offset_x:f64, pub head_offset_y:f64, pub paw_scale:f64 }
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all="camelCase", deny_unknown_fields)]
pub struct SkinPackage {
    pub version:u32, pub style:String, pub id:String, pub name:String, pub created_at:String,
    pub atlas_data_url:String, pub parts:BTreeMap<String,Crop>, pub rig:Rig,
}
fn valid_id(id:&str)->bool {
    id.starts_with("diy-") && id.len()>4 && id.len()<=36 && id.bytes().all(|b|b.is_ascii_lowercase()||b.is_ascii_digit()||b==b'-'||b==b'_')
}
fn in_range(v:f64,min:f64,max:f64)->bool { v.is_finite() && v>=min && v<=max }
fn decode_data_url(data:&str, png_only:bool)->Result<(Vec<u8>, image::ImageFormat),String> {
    if data.len()>MAX_IMAGE*4/3+128 { return Err("图片超过 10 MB。".into()); }
    let (header,encoded)=data.split_once(',').ok_or("图片编码无效。")?;
    let format=match header {
        "data:image/png;base64"=>image::ImageFormat::Png,
        "data:image/jpeg;base64" if !png_only=>image::ImageFormat::Jpeg,
        "data:image/webp;base64" if !png_only=>image::ImageFormat::WebP,
        _=>return Err("请选择 PNG、JPG 或 WebP 图片。".into()),
    };
    let bytes=STANDARD.decode(encoded).map_err(|_|"图片编码无效。")?;
    if bytes.len()>MAX_IMAGE { return Err("图片超过 10 MB。".into()); }
    Ok((bytes,format))
}
fn read_image(bytes:&[u8], format:image::ImageFormat)->Result<image::RgbaImage,String> {
    let mut reader=image::ImageReader::with_format(Cursor::new(bytes),format);
    let mut limits=image::Limits::default();
    limits.max_image_width=Some(4096); limits.max_image_height=Some(4096); limits.max_alloc=Some(128*1024*1024);
    reader.limits(limits);
    reader.decode().map(|i|i.into_rgba8()).map_err(|_|"图片损坏或尺寸超过 4096 像素。".into())
}
pub fn validate(s:&SkinPackage)->Result<(),String> {
    if s.version!=1 || s.style!="loose-crayon-v1" {return Err("不支持这个皮肤包版本或画风。".into());}
    if !valid_id(&s.id) {return Err("小猫编号无效。".into());}
    if s.name.trim().is_empty() || s.name.chars().count()>60 || time::OffsetDateTime::parse(&s.created_at, &time::format_description::well_known::Rfc3339).is_err() {return Err("小猫名称或日期无效。".into());}
    if s.parts.len()!=6 {return Err("动作素材必须包含六个部件。".into());}
    let r=&s.rig;
    if !in_range(r.head_scale,0.65,1.4)||!in_range(r.body_scale,0.65,1.4)||!in_range(r.paw_scale,0.65,1.4)||!in_range(r.head_offset_x,-35.,35.)||!in_range(r.head_offset_y,-35.,35.) {return Err("动作校准超出允许范围。".into());}
    let (bytes,format)=decode_data_url(&s.atlas_data_url,true)?;
    let img=read_image(&bytes,format)?;
    if img.width()<64 || img.height()<64 {return Err("素材尺寸至少为 64 像素。".into());}
    for key in PARTS {
        let c=s.parts.get(key).ok_or("缺少动作部件。")?;
        if !in_range(c.x,0.,1.)||!in_range(c.y,0.,1.)||!in_range(c.w,0.01,1.)||!in_range(c.h,0.01,1.)||c.x+c.w>1.000001||c.y+c.h>1.000001 {return Err(format!("{key} 裁切范围无效。"));}
        let (x0,y0)=((c.x*img.width() as f64) as u32,(c.y*img.height() as f64) as u32);
        let (x1,y1)=(((c.x+c.w)*img.width() as f64) as u32,((c.y+c.h)*img.height() as f64) as u32);
        let mut visible=0;let mut clear=0;let mut total=0;
        for y in y0..y1.min(img.height()) {for x in x0..x1.min(img.width()) {let a=img.get_pixel(x,y)[3];total+=1;if a>96{visible+=1;}if a<16{clear+=1;}}}
        if total==0||visible as f64/(total as f64)<0.006||clear as f64/(total as f64)<0.015 {return Err(format!("{key} 为空或缺少透明背景。"));}
    }
    Ok(())
}
fn skin_path(root:&Path,id:&str)->Result<PathBuf,String> {
    if !valid_id(id){return Err("小猫编号无效。".into());}
    Ok(root.join(format!("{id}.catpack")))
}
fn write_atomic(path:&Path,bytes:&[u8])->Result<(),String> {
    let parent=path.parent().ok_or("保存位置无效。")?;
    fs::create_dir_all(parent).map_err(|_|"无法创建保存目录。")?;
    let temp=path.with_extension(format!("{}.{}.tmp",std::process::id(),NEXT_WRITE.fetch_add(1,Ordering::Relaxed)));
    fs::write(&temp,bytes).and_then(|_|fs::rename(&temp,path)).map_err(|_|{let _=fs::remove_file(&temp);"保存失败，请检查磁盘空间及权限。".to_string()})
}
fn load_at(root:&Path,id:&str)->Result<SkinPackage,String> {
    let path=skin_path(root,id)?;
    let meta=fs::symlink_metadata(&path).map_err(|_|"找不到这只小猫。")?;
    if !meta.is_file()||meta.file_type().is_symlink()||meta.len()>MAX_FILE as u64 {return Err("皮肤包文件无效。".into());}
    let bytes=fs::read(path).map_err(|_|"无法读取小猫。")?;
    let s:SkinPackage=serde_json::from_slice(&bytes).map_err(|_|"皮肤包损坏。")?;
    if s.id!=id {return Err("皮肤包编号不匹配。".into());}
    validate(&s)?;Ok(s)
}
fn save_at(root:&Path,s:&SkinPackage)->Result<(),String> {
    validate(s)?;
    let path=skin_path(root,&s.id)?;
    if fs::symlink_metadata(&path).is_ok_and(|m|m.file_type().is_symlink()){return Err("保存位置无效。".into());}
    let bytes=serde_json::to_vec(s).map_err(|_|"无法编码皮肤包。")?;
    if bytes.len()>MAX_FILE{return Err("皮肤包超过 20 MB。".into());}
    write_atomic(&path,&bytes)
}
fn root() -> PathBuf { crate::config::app_data_dir().join("diy-skins") }
pub fn load_saved_skin(id: &str) -> Result<SkinPackage, String> { load_at(&root(), id) }
pub fn has_saved_skin(id: &str) -> bool { skin_path(&root(), id).is_ok_and(|p|p.exists()) }
#[tauri::command]
pub async fn load_custom_skin(id: String) -> Result<SkinPackage, String> {
    tauri::async_runtime::spawn_blocking(move || load_saved_skin(&id)).await.map_err(|_|"小猫读取失败。".to_string())?
}

fn read_local(path: &str, extension: &[&str], limit: usize) -> Result<Vec<u8>, String> {
    let path=Path::new(path);
    if !path.is_absolute() { return Err("请提供此电脑上的绝对路径；远程 AI 无法直接读取其他电脑的文件。".into()); }
    if !path.extension().and_then(|x|x.to_str()).is_some_and(|x|extension.iter().any(|e|x.eq_ignore_ascii_case(e))) {return Err("文件类型不支持，请选择 PNG 或 .catpack。".into());}
    let meta=fs::symlink_metadata(path).map_err(|_|"文件不存在，AI 生成的文件需要先保存到运行 KuroCat 的电脑。")?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len()>limit as u64 {return Err("文件无效或超过大小限制。".into());}
    let mut bytes=vec![];
    fs::File::open(path).map_err(|_|"无法打开文件。")?.take((limit+1) as u64).read_to_end(&mut bytes).map_err(|_|"文件读取失败。")?;
    if bytes.len()>limit {return Err("文件超过大小限制。".into());}
    Ok(bytes)
}
fn png_url(path: &str) -> Result<String,String> {
    let bytes=read_local(path,&["png"],MAX_IMAGE)?;
    Ok(format!("data:image/png;base64,{}",STANDARD.encode(bytes)))
}
fn required<'a>(args: &'a Value, key: &str) -> Result<&'a str,String> {
    args.get(key).and_then(Value::as_str).filter(|s|!s.trim().is_empty()).ok_or_else(||format!("缺少 {key} 字符串。"))
}
fn grid_parts() -> BTreeMap<String,Crop> {
    PARTS.iter().enumerate().map(|(i,k)|(k.to_string(),Crop{x:(i%2) as f64/2.,y:(i/2) as f64/3.,w:0.5,h:1./3.})).collect()
}
fn default_rig() -> Rig { Rig{head_scale:1.,body_scale:1.,head_offset_x:0.,head_offset_y:0.,paw_scale:1.} }
fn patch(s: &mut SkinPackage, args: &Value) -> Result<(),String> {
    if let Some(value)=args.get("name") {s.name=value.as_str().ok_or("name 必须是字符串。")?.trim().to_string();}
    for key in ["rig","parts"] {
        if let Some(value)=args.get(key) {
            let changes=value.as_object().ok_or_else(||format!("{key} 必须是对象。"))?;
            let mut merged=if key=="rig" {serde_json::to_value(&s.rig)} else {serde_json::to_value(&s.parts)}.map_err(|_|"配置编码失败。")?;
            for (k,v) in changes {
                if merged.get(k).is_none() {return Err(format!("不支持 {key}.{k}。"));}
                if key=="parts" {
                    let crop=v.as_object().ok_or("裁切配置必须是对象。")?;
                    for (field,value) in crop {if merged[k].get(field).is_none(){return Err(format!("不支持裁切属性 {field}。"));}merged[k][field]=value.clone();}
                } else {merged[k]=v.clone();}
            }
            if key=="rig" {s.rig=serde_json::from_value(merged).map_err(|_|"动作校准格式无效。")?;}
            else {s.parts=serde_json::from_value(merged).map_err(|_|"裁切格式无效。")?;}
        }
    }
    validate(s)
}
fn summary(s: &SkinPackage) -> Value {
    let mut hash=std::collections::hash_map::DefaultHasher::new();
    serde_json::to_string(s).unwrap_or_default().hash(&mut hash);
    json!({"id":s.id,"name":s.name,"createdAt":s.created_at,"style":s.style,"parts":s.parts,"rig":s.rig,"revision":format!("{:016x}",hash.finish()),"renderMode":"2d"})
}
fn persist(s: &SkinPackage, activate: bool) -> Result<Value,String> {
    let root=root();let path=skin_path(&root,&s.id)?;
    if !path.exists() && root.exists() && fs::read_dir(&root).map_err(|_|"无法读取小猫库。")?.flatten().filter(|e|e.path().extension().is_some_and(|x|x=="catpack")).count()>=64 {return Err("小猫库已满（64 只），请先导出并移除暂时不用的小猫。".into());}
    let previous=if path.exists(){Some(serde_json::to_vec(&load_at(&root,&s.id)?).map_err(|_|"现有皮肤包损坏。")?)}else{None};
    save_at(&root,s)?;
    let info=summary(s);
    let mut cat=crate::cats::all_cats().into_iter().find(|c|c.id==s.id)
        .unwrap_or_else(||serde_json::from_value(json!({"id":s.id,"name":s.name,"furColor":"#808080"})).unwrap());
    cat.name=s.name.clone();cat.sketch_skin=Some(s.id.clone());cat.sketch_revision=info["revision"].as_str().map(str::to_string);cat.emoji=Some("✎".into());
    if let Err(e)=crate::cats::upsert_custom_cat(cat) {
        if let Some(bytes)=previous {let _=write_atomic(&path,&bytes);}else{let _=fs::remove_file(&path);}
        return Err(format!("小猫配置保存失败，素材已回退：{e}"));
    }
    if activate {
        let mut settings=crate::config::load_settings();settings.cat_breed=s.id.clone();settings.render_mode="2d".into();
        crate::config::try_save_settings(&settings).map_err(|_|"素材已保存，但切换失败，请使用 switch_cat 重试。")?;
    }
    Ok(json!({"status":"saved","activated":activate,"skin":info}))
}
fn import(args: &Value) -> Result<Value,String> {
    let path=required(args,"path")?;
    let bytes=read_local(path,&["png","catpack"],MAX_FILE)?;
    let mut skin:SkinPackage=if Path::new(path).extension().is_some_and(|x|x.eq_ignore_ascii_case("catpack")) {
        serde_json::from_slice(&bytes).map_err(|_|"皮肤包格式无效，不能包含未知字段。")?
    } else {
        if bytes.len()>MAX_IMAGE {return Err("PNG 超过 10 MB。".into());}
        let now=time::OffsetDateTime::now_utc();
        SkinPackage{version:1,style:"loose-crayon-v1".into(),id:format!("diy-{:x}",now.unix_timestamp_nanos()),name:required(args,"name")?.to_string(),created_at:now.format(&time::format_description::well_known::Rfc3339).map_err(|_|"无法读取时间。")?,atlas_data_url:format!("data:image/png;base64,{}",STANDARD.encode(bytes)),parts:grid_parts(),rig:default_rig()}
    };
    if let Some(id)=args.get("id"){skin.id=id.as_str().ok_or("id 必须是字符串。")?.to_string();}
    patch(&mut skin,args)?;
    let path=skin_path(&root(),&skin.id)?;
    if (path.exists()||crate::cats::cat_exists(&skin.id)) && !args.get("overwrite").and_then(Value::as_bool).unwrap_or(false) {return Err("这个编号已存在。请指定新的 diy- 编号，或明确传 overwrite: true 更新现有小猫。".into());}
    persist(&skin,args.get("activate").and_then(Value::as_bool).unwrap_or(true))
}
fn update(args: &Value) -> Result<Value,String> {
    let mut skin=load_saved_skin(required(args,"id")?)?;
    if let Some(path)=args.get("atlasPath") {skin.atlas_data_url=png_url(path.as_str().ok_or("atlasPath 必须是绝对路径。")?)?;}
    patch(&mut skin,args)?;
    persist(&skin,args.get("activate").and_then(Value::as_bool).unwrap_or(false))
}
fn export(args: &Value) -> Result<Value,String> {
    let skin=load_saved_skin(required(args,"id")?)?;
    let path=Path::new(required(args,"path")?);
    if !path.is_absolute() || !path.extension().is_some_and(|x|x.eq_ignore_ascii_case("catpack")){return Err("请提供以 .catpack 结尾的导出绝对路径。".into());}
    let bytes=serde_json::to_vec_pretty(&skin).map_err(|_|"皮肤包编码失败。")?;
    if args.get("overwrite").and_then(Value::as_bool).unwrap_or(false) {
        if fs::symlink_metadata(path).is_ok_and(|m|!m.is_file()||m.file_type().is_symlink()){return Err("导出位置无效。".into());}
        write_atomic(path,&bytes)?;
    } else {
        let mut file=fs::OpenOptions::new().write(true).create_new(true).open(path).map_err(|_|"导出位置不可写或文件已存在；不会自动覆盖。")?;
        if file.write_all(&bytes).is_err(){let _=fs::remove_file(path);return Err("导出失败。".into());}
    }
    Ok(json!({"status":"exported","path":path,"skin":summary(&skin),"containsSourcePhotos":false,"containsCredentials":false}))
}
pub fn delete_saved_skin(id: &str) -> Result<bool,String> {
    let _lock=MUTATIONS.lock().map_err(|_|"小猫库暂不可用。")?;
    let root=root();let path=skin_path(&root,id)?;let _=load_at(&root,id)?;
    let trash=root.join(".trash");fs::create_dir_all(&trash).map_err(|_|"无法创建回收目录。")?;
    let stamp=SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();let target=trash.join(format!("{id}-{stamp}.catpack"));
    fs::rename(&path,&target).map_err(|_|"无法移入回收目录。")?;
    if let Err(e)=crate::cats::delete_custom_cat(id){let _=fs::rename(target,path);return Err(e);}
    Ok(true)
}
pub fn guide() -> Value {
    json!({
        "generationProvidedBy":"connected AI client",
        "workflow":["Look at the user's reference photos and read this guide.","Generate one transparent six-part PNG with the connected AI client's image tools, or ask for an existing PNG/catpack. MCP itself does not generate images or call a paid provider.","Save the file on the computer running KuroCat, then import_custom_skin with its absolute path. Use activate:true for live desktop animation preview.","Use inspect_custom_skin for metadata and atlas image review. This image is the source atlas, not an assembled animation preview.","For proportions, update_custom_skin with a partial rig; for visual changes, regenerate the atlas and update with atlasPath. Always preserve the individual cat's features.","Export a portable catpack to share. switch_cat and delete_cat work for both artwork and original parameter cats."],
        "prompt":include_str!("../../docs/atlas-prompt.txt"),
        "partsOrder":PARTS,"defaultParts":grid_parts(),"defaultRig":default_rig(),
        "rigLimits":{"headScale":[0.65,1.4],"bodyScale":[0.65,1.4],"pawScale":[0.65,1.4],"headOffsetX":[-35,35],"headOffsetY":[-35,35]},
        "cropUnits":"Normalized 0..1 x/y/w/h; updates merge individual fields. Each crop needs visible pixels and genuine transparent padding.",
        "limits":{"pngBytes":MAX_IMAGE,"packageBytes":MAX_FILE,"maxDimension":4096,"libraryCats":64},
        "idFormat":"diy- plus 1..32 lowercase letters, digits, hyphens or underscores; omit for auto ID when importing PNG.",
        "compatibility":"Custom artwork supports 2D and existing keyboard/mouse/blink animations. It does not create 3D geometry or arbitrary new skeletons. Style is a default generation prompt, not a pixel-level restriction; other artwork can be imported if it follows the part layout.",
        "fileAccess":"Only explicit absolute local PNG/catpack paths. No URLs, raw photos, API keys or executable scripts. Remote clients must transfer files to the KuroCat computer first. Use tools to pass paths, not megabytes of base64."
    })
}
pub fn call(name: &str, args: &Value) -> Result<Value,String> {
    if name=="get_custom_skin_guide" {return Ok(guide());}
    if name=="inspect_custom_skin" {
        let skin=load_saved_skin(required(args,"id")?)?;
        let mut result=json!({"content":[{"type":"text","text":serde_json::to_string_pretty(&summary(&skin)).unwrap()}],"isError":false});
        if args.get("includeImage").and_then(Value::as_bool).unwrap_or(true) {
            result["content"].as_array_mut().unwrap().push(json!({"type":"image","mimeType":"image/png","data":skin.atlas_data_url.split_once(',').unwrap().1}));
        }
        return Ok(result);
    }
    let _lock=MUTATIONS.lock().map_err(|_|"小猫库暂不可用。")?;
    match name {"import_custom_skin"=>import(args),"update_custom_skin"=>update(args),"export_custom_skin"=>export(args),_=>Err("Unknown custom skin tool".into())}
}

pub fn tool_definitions() -> Vec<Value> {
    let crop=json!({"type":"object","additionalProperties":false,"properties":{"x":{"type":"number","minimum":0,"maximum":1},"y":{"type":"number","minimum":0,"maximum":1},"w":{"type":"number","minimum":0.01,"maximum":1},"h":{"type":"number","minimum":0.01,"maximum":1}}});
    let mut crops=serde_json::Map::new();for part in PARTS {crops.insert(part.into(),crop.clone());}
    let mut rig=serde_json::Map::new();for key in ["headScale","bodyScale","pawScale"] {rig.insert(key.into(),json!({"type":"number","minimum":0.65,"maximum":1.4}));}for key in ["headOffsetX","headOffsetY"] {rig.insert(key.into(),json!({"type":"number","minimum":-35,"maximum":35}));}
    let shared=json!({"name":{"type":"string","minLength":1,"maxLength":60},"rig":{"type":"object","properties":rig,"additionalProperties":false},"parts":{"type":"object","properties":crops,"additionalProperties":false},"activate":{"type":"boolean"}});
    let id=json!({"type":"string","pattern":"^diy-[a-z0-9_-]{1,32}$"});
    let path=json!({"type":"string","description":"Explicit absolute path on the computer running KuroCat; not a URL or path on a remote AI server."});
    let mut import=shared.as_object().unwrap().clone();import.insert("path".into(),path.clone());import.insert("id".into(),id.clone());import.insert("overwrite".into(),json!({"type":"boolean","default":false}));
    let mut update=shared.as_object().unwrap().clone();update.insert("id".into(),id.clone());update.insert("atlasPath".into(),path.clone());
    vec![
        json!({"name":"get_custom_skin_guide","description":"Read this FIRST to create a freely drawn cat from photos. Returns default loose hand-drawn style, six-part atlas prompt, limits and workflow. Artwork generation is provided by the connected AI client, not KuroCat. Original upsert_cat is only for parameter colors/markings.","inputSchema":{"type":"object","properties":{},"additionalProperties":false},"annotations":{"readOnlyHint":true}}),
        json!({"name":"import_custom_skin","description":"Import a locally saved transparent six-part PNG or portable catpack. Freely customizes outline, face, coat and drawing style while retaining typing/mouse/blink actions. PNG requires name; id auto-generated if omitted. Refuses existing IDs unless overwrite:true. activate defaults true. Read get_custom_skin_guide first.","inputSchema":{"type":"object","required":["path"],"properties":import,"additionalProperties":false},"annotations":{"readOnlyHint":false,"destructiveHint":false}}),
        json!({"name":"update_custom_skin","description":"Edit an existing artwork cat without losing its identity. Partial rig/crop fields are merged. To change face, fur or drawing style, regenerate artwork with the AI client's image tools and provide atlasPath. name optional; activate defaults false, current active cat still reloads updated artwork.","inputSchema":{"type":"object","required":["id"],"properties":update,"additionalProperties":false},"annotations":{"readOnlyHint":false,"destructiveHint":true}}),
        json!({"name":"inspect_custom_skin","description":"Return saved cat metadata, rig/crops and optionally the source PNG atlas as an MCP image (default true). This is not an assembled animation preview; switch_cat gives live desktop preview.","inputSchema":{"type":"object","required":["id"],"properties":{"id":id,"includeImage":{"type":"boolean","default":true}},"additionalProperties":false},"annotations":{"readOnlyHint":true}}),
        json!({"name":"export_custom_skin","description":"Export a self-contained catpack to an explicit absolute .catpack file path for sharing/backup. No source photos, credentials or executable scripts. Destination parent must exist. Refuses overwrite unless overwrite:true.","inputSchema":{"type":"object","required":["id","path"],"properties":{"id":id,"path":path,"overwrite":{"type":"boolean","default":false}},"additionalProperties":false},"annotations":{"readOnlyHint":false,"destructiveHint":false}})
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> SkinPackage {
        let mut img=image::RgbaImage::new(120,120);
        for row in 0..3 {for col in 0..2 {for y in row*40+5..row*40+35 {for x in col*60+5..col*60+55 {img.put_pixel(x,y,image::Rgba([20,40,100,255]));}}}}
        let mut buf=Cursor::new(Vec::new());image::DynamicImage::ImageRgba8(img).write_to(&mut buf,image::ImageFormat::Png).unwrap();
        SkinPackage{version:1,style:"loose-crayon-v1".into(),id:"diy-test".into(),name:"黑白猫".into(),created_at:"2026-11-26T00:00:00Z".into(),atlas_data_url:format!("data:image/png;base64,{}",STANDARD.encode(buf.into_inner())),parts:grid_parts(),rig:default_rig()}
    }
    #[test]
    fn package_roundtrip_is_bounded_and_rejects_path_escape() {
        let root=std::env::temp_dir().join(format!("kurocat-skin-test-{}-{}",std::process::id(),NEXT_WRITE.fetch_add(1,Ordering::Relaxed)));
        let s=sample();save_at(&root,&s).unwrap();let loaded=load_at(&root,&s.id).unwrap();assert_eq!(loaded.atlas_data_url,s.atlas_data_url);
        assert!(load_at(&root,"../../settings").is_err());
        let mut bad=s.clone();bad.version=9;assert!(save_at(&root,&bad).is_err());
        bad=s.clone();bad.parts.remove("tail");assert!(validate(&bad).is_err());
        bad=s.clone();bad.parts.get_mut("headOpen").unwrap().x=0.99;assert!(validate(&bad).is_err());
        bad=s.clone();bad.created_at="yesterday".into();assert!(validate(&bad).is_err());
        assert_eq!(load_at(&root,&s.id).unwrap().atlas_data_url,s.atlas_data_url);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn partial_calibration_preserves_other_parts_and_changes_revision() {
        let mut s=sample();let old=summary(&s);let prior=serde_json::to_value(&s.parts).unwrap();
        patch(&mut s,&json!({"rig":{"headScale":1.12},"parts":{"headOpen":{"x":0.01,"w":0.49}}})).unwrap();
        assert_eq!(s.rig.head_scale,1.12);assert_eq!(s.rig.paw_scale,1.);assert_eq!(s.parts["tail"].x,prior["tail"]["x"].as_f64().unwrap());
        assert_ne!(summary(&s)["revision"],old["revision"]);
        assert!(patch(&mut s,&json!({"rig":{"unknown":1}})).is_err());
        assert!(patch(&mut s,&json!({"parts":{"tail":{"url":"https://bad"}}})).is_err());
        assert!(patch(&mut s,&json!({"rig":{"headScale":100}})).is_err());
    }
    #[test]
    fn opaque_or_empty_atlas_is_rejected() {
        let mut s=sample();let mut buf=Cursor::new(Vec::new());image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(120,120,image::Rgba([1,2,3,255]))).write_to(&mut buf,image::ImageFormat::Png).unwrap();
        s.atlas_data_url=format!("data:image/png;base64,{}",STANDARD.encode(buf.into_inner()));assert!(validate(&s).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn local_import_rejects_symlinks_and_remote_urls() {
        use std::os::unix::fs::symlink;
        let path=std::env::temp_dir().join(format!("kurocat-symlink-{}.catpack",std::process::id()));
        let _=fs::remove_file(&path);symlink("/etc/hosts",&path).unwrap();
        assert!(read_local(path.to_str().unwrap(),&["catpack"],MAX_FILE).is_err());
        assert!(read_local("https://example.com/cat.png",&["png"],MAX_IMAGE).is_err());
        fs::remove_file(path).unwrap();
    }
}
