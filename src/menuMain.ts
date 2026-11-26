import './style.css';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { DEFAULT_TAIL_KEY_IDS, type KeyboardOrientation, type TailKeyMode } from './catEngine';
import { DEFAULT_BUILTIN_CATS, type CatSkinConfig } from './catSkins';
import type { KeyboardLayoutId } from './keyboardLayout';

interface AppSettings {
  x: number | null;
  y: number | null;
  scale: number;
  alwaysOnTop: boolean;
  clickThroughLocked: boolean;
  showKeyBubbles: boolean;
  soundEnabled: boolean;
  autostart: boolean;
  catBreed: string;
  mcpEnabled: boolean;
  mcpPort: number;
  keyboardLayout: KeyboardLayoutId;
  keyboardOrientation: KeyboardOrientation;
  privacyPasswordGuard: boolean;
  tailKeyMode: TailKeyMode;
  tailKeyIds: string[];
  obsEnabled: boolean;
  obsPort: number;
  obsHidePet: boolean;
  renderMode?: '2d' | '3d';
  collarColor?: 'none' | 'red' | 'blue' | 'pink' | 'green';
  headphoneMode?: 'auto' | 'on' | 'off';
}

interface UpdateInfo {
  currentVersion: string;
  latestVersion: string;
  hasUpdate: boolean;
  releaseUrl: string;
  assetName: string | null;
  downloadUrl: string | null;
  assetSha256: string | null;
  assetSize: number | null;
  installKind: string;
  notesSummary: string;
}

interface UpdateProgressEvent {
  stage: 'checking' | 'available' | 'up_to_date' | 'downloading' | 'verifying' | 'installing' | 'error';
  message: string;
  info?: UpdateInfo | null;
}

type MenuTabId = 'appearance' | 'system';

const isMacPlatform = (): boolean =>
  /Mac/i.test(navigator.platform) || /Macintosh/i.test(navigator.userAgent);

const TAIL_MODE_LABELS: Record<TailKeyMode, string> = {
  smart: '智能第三只手',
  custom: '自定义按键',
  edge: '近尾边缘区',
  off: '仅摆动',
};

let settings: AppSettings = {
  x: null,
  y: null,
  scale: 1,
  alwaysOnTop: true,
  clickThroughLocked: false,
  showKeyBubbles: true,
  soundEnabled: true,
  autostart: false,
  catBreed: 'black',
  mcpEnabled: true,
  mcpPort: 27420,
  keyboardLayout: '65',
  keyboardOrientation: 'cat',
  privacyPasswordGuard: true,
  tailKeyMode: 'smart',
  tailKeyIds: [...DEFAULT_TAIL_KEY_IDS],
  obsEnabled: false,
  obsPort: 27421,
  obsHidePet: false,
  renderMode: '3d',
  collarColor: 'none',
  headphoneMode: 'off',
};

let catList: CatSkinConfig[] = [...DEFAULT_BUILTIN_CATS];
let sliderDragging = false;
let currentAppVersion = '5.1.0';
let cachedUpdateInfo: UpdateInfo | null = null;
let updateBusy = false;
let activeTab: MenuTabId = 'appearance';

const menuEl = document.getElementById('context-menu') as HTMLDivElement;
const tabBodyEl = document.getElementById('menu-tab-body') as HTMLDivElement | null;
const breedPillsEl = document.getElementById('breed-pills') as HTMLDivElement;
const scaleSliderEl = document.getElementById('scale-slider') as HTMLInputElement;
const scaleValueEl = document.getElementById('scale-value') as HTMLButtonElement;
const tailModeBadgeEl = document.getElementById('tail-mode-badge') as HTMLSpanElement;
const tailKeysCountEl = document.getElementById('tail-keys-count') as HTMLSpanElement;
const btnObsHideEl = document.getElementById('btn-obs-hide') as HTMLButtonElement;
const btnCheckUpdateEl = document.getElementById('btn-check-update') as HTMLButtonElement | null;

function switchTab(nextTab: MenuTabId): void {
  activeTab = nextTab;
  document.querySelectorAll<HTMLButtonElement>('[data-tab]').forEach((btn) => {
    const isActive = btn.dataset.tab === activeTab;
    btn.classList.toggle('active', isActive);
    btn.setAttribute('aria-selected', isActive ? 'true' : 'false');
  });
  document.querySelectorAll<HTMLElement>('[data-tab-panel]').forEach((panel) => {
    panel.classList.toggle('hidden', panel.dataset.tabPanel !== activeTab);
  });
  if (tabBodyEl) {
    tabBodyEl.scrollTop = 0;
  }
}

function renderFooterUpdateState(_customLabel?: string, _ready = false): void {
  const hint = document.getElementById('update-hint');
  if (btnCheckUpdateEl) {
    btnCheckUpdateEl.disabled = true;
    btnCheckUpdateEl.classList.remove('update-ready');
  }
  if (hint) hint.textContent = '官方更新已停用';
}

function closeMenu(): void {
  void invoke('close_context_menu');
}

function updateScaleSliderVisual(scaleVal: number): void {
  const pctNum = Math.round(Math.max(20, Math.min(150, scaleVal * 100)));
  if (scaleSliderEl) {
    scaleSliderEl.value = String(pctNum);
    const fillPct = ((pctNum - 20) / (150 - 20)) * 100;
    scaleSliderEl.style.setProperty('--pct', `${fillPct.toFixed(1)}%`);
  }
  if (scaleValueEl) {
    scaleValueEl.textContent = `${pctNum}%`;
  }
}

function renderBreedPills(): void {
  if (!breedPillsEl) return;
  breedPillsEl.innerHTML = '';
  for (const cat of catList) {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'breed-pill';
    btn.dataset.breed = cat.id;
    const emoji = cat.emoji ? `${cat.emoji} ` : '🐱 ';
    btn.textContent = `${emoji}${cat.name}`;
    const accent = cat.accentColor || (cat.id === 'ragdoll' ? '#7ac7ff' : '#c8f538');
    btn.style.setProperty('--pill-accent', accent);
    btn.classList.toggle('active', cat.id === settings.catBreed);
    breedPillsEl.appendChild(btn);
  }
}

function syncUI(): void {
  const setCheck = (id: string, on: boolean) => {
    const el = document.getElementById(id);
    if (el) el.textContent = on ? '✓' : '';
  };

  setCheck('chk-privacy', settings.privacyPasswordGuard !== false);
  setCheck('chk-bubbles', settings.showKeyBubbles);
  setCheck('chk-lock', settings.clickThroughLocked);
  setCheck('chk-sound', settings.soundEnabled);
  setCheck('chk-top', settings.alwaysOnTop);
  setCheck('chk-autostart', settings.autostart);
  setCheck('chk-mcp', settings.mcpEnabled);
  setCheck('chk-obs', Boolean(settings.obsEnabled));
  setCheck('chk-obs-hide', Boolean(settings.obsEnabled && settings.obsHidePet));

  if (btnObsHideEl) {
    btnObsHideEl.classList.toggle('hidden', !settings.obsEnabled);
  }
  const obsHint = document.getElementById('obs-copy-hint');
  if (obsHint && !obsHint.classList.contains('copied')) {
    obsHint.textContent = settings.obsEnabled ? `:${settings.obsPort || 27421}` : '免抠像';
  }

  if (!sliderDragging) {
    updateScaleSliderVisual(settings.scale);
  }

  const kbLayout: KeyboardLayoutId = settings.keyboardLayout === '108' ? '108' : '65';
  const kbOrient: KeyboardOrientation = settings.keyboardOrientation === 'viewer' ? 'viewer' : 'cat';
  document.querySelectorAll<HTMLButtonElement>('[data-kblayout]').forEach((btn) => {
    btn.classList.toggle('active', btn.dataset.kblayout === kbLayout);
  });
  document.querySelectorAll<HTMLButtonElement>('[data-kborient]').forEach((btn) => {
    btn.classList.toggle('active', btn.dataset.kborient === kbOrient);
  });

  const tailMode: TailKeyMode =
    settings.tailKeyMode === 'custom' ||
    settings.tailKeyMode === 'edge' ||
    settings.tailKeyMode === 'off'
      ? settings.tailKeyMode
      : 'smart';
  const tailIds = Array.isArray(settings.tailKeyIds) ? settings.tailKeyIds : [...DEFAULT_TAIL_KEY_IDS];
  document.querySelectorAll<HTMLButtonElement>('[data-tailmode]').forEach((btn) => {
    btn.classList.toggle('active', btn.dataset.tailmode === tailMode);
  });
  if (tailModeBadgeEl) {
    tailModeBadgeEl.textContent = TAIL_MODE_LABELS[tailMode];
  }
  if (tailKeysCountEl) {
    tailKeysCountEl.textContent = `${tailIds.length} 键`;
  }

  const activeCat = catList.find((c) => c.id === settings.catBreed) ?? catList[0];
  renderBreedPills();

  const renderMode = activeCat?.sketchSkin || settings.renderMode === '2d' ? '2d' : '3d';
  const collarColor =
    settings.collarColor === 'red' ||
    settings.collarColor === 'blue' ||
    settings.collarColor === 'pink' ||
    settings.collarColor === 'green'
      ? settings.collarColor
      : 'none';
  document.querySelectorAll<HTMLButtonElement>('[data-rendermode]').forEach((btn) => {
    btn.classList.toggle('active', btn.dataset.rendermode === renderMode);
    btn.disabled = Boolean(activeCat?.sketchSkin && btn.dataset.rendermode === '3d');
    btn.title = btn.disabled ? '手绘皮肤使用 2D 动作模式' : '';
  });
  document.querySelectorAll<HTMLButtonElement>('[data-collar]').forEach((btn) => {
    btn.classList.toggle('active', btn.dataset.collar === collarColor);
  });
  const collarRowEl = document.getElementById('collar-row');
  if (collarRowEl) {
    collarRowEl.classList.toggle('hidden', renderMode === '2d' || settings.catBreed === 'dafeiyu');
  }

  const headphoneMode =
    settings.headphoneMode === 'auto' || settings.headphoneMode === 'on'
      ? settings.headphoneMode
      : 'off';
  document.querySelectorAll<HTMLButtonElement>('[data-headphonemode]').forEach((btn) => {
    btn.classList.toggle('active', btn.dataset.headphonemode === headphoneMode);
  });
  const headphoneRowEl = document.getElementById('headphone-row');
  if (headphoneRowEl) {
    headphoneRowEl.classList.toggle('hidden', settings.catBreed === 'dafeiyu');
  }

  const headerEl = document.getElementById('menu-header');
  if (headerEl && activeCat) {
    const emoji = activeCat.emoji || '🐱';
    headerEl.textContent = `${emoji} KuroCat ${activeCat.name}`;
    headerEl.style.color = activeCat.accentColor || '#c8f538';
  }
}

async function applySettingsPatch(patch: Partial<AppSettings>): Promise<void> {
  settings = { ...settings, ...patch };
  syncUI();
  settings = await invoke<AppSettings>('update_settings', { patch: settings });
  syncUI();
}

async function copyText(text: string): Promise<void> {
  if (navigator.clipboard && navigator.clipboard.writeText) {
    await navigator.clipboard.writeText(text);
    return;
  }
  const ta = document.createElement('textarea');
  ta.value = text;
  ta.style.position = 'fixed';
  ta.style.opacity = '0';
  document.body.appendChild(ta);
  ta.select();
  document.execCommand('copy');
  document.body.removeChild(ta);
}

async function copyMcpConfig(): Promise<void> {
  const hint = document.getElementById('mcp-copy-hint');
  try {
    const jsonText = await invoke<string>('get_mcp_config');
    await copyText(jsonText);
    if (hint) {
      hint.textContent = '✓ 已复制';
      hint.classList.add('copied');
      setTimeout(() => {
        hint.textContent = 'AI 捏猫';
        hint.classList.remove('copied');
      }, 1800);
    }
  } catch (err) {
    console.error('Failed to copy MCP config:', err);
  }
}

async function copyObsUrl(): Promise<void> {
  const hint = document.getElementById('obs-copy-hint');
  try {
    if (!settings.obsEnabled) {
      await applySettingsPatch({ obsEnabled: true });
    }
    const url = await invoke<string>('get_obs_url');
    await copyText(url);
    if (hint) {
      hint.textContent = '✓ 已复制';
      hint.classList.add('copied');
      setTimeout(() => {
        hint.classList.remove('copied');
        hint.textContent = settings.obsEnabled ? `:${settings.obsPort || 27421}` : '免抠像';
      }, 2000);
    }
  } catch (err) {
    console.error('Failed to copy OBS url:', err);
  }
}

async function createDesktopShortcut(): Promise<void> {
  const hint = document.getElementById('shortcut-hint');
  try {
    await invoke<string>('create_desktop_shortcut');
    if (hint) {
      hint.textContent = '✓ 已创建';
      hint.classList.add('copied');
      setTimeout(() => {
        hint.textContent = 'KuroCat';
        hint.classList.remove('copied');
      }, 1800);
    }
  } catch (err) {
    console.error('Failed to create shortcut:', err);
    if (hint) {
      hint.textContent = '失败';
      setTimeout(() => {
        hint.textContent = 'KuroCat';
      }, 1800);
    }
  }
}

async function triggerManualUpdateCheck(): Promise<void> {
  if (updateBusy) return;
  // If we already know an update is available, clicking immediately installs (or opens release page).
  if (cachedUpdateInfo?.hasUpdate) {
    if (cachedUpdateInfo.downloadUrl) {
      updateBusy = true;
      renderFooterUpdateState(`下载 v${cachedUpdateInfo.latestVersion}...`, true);
      closeMenu();
      try {
        await invoke('install_update', { info: cachedUpdateInfo });
      } catch {
        updateBusy = false;
        renderFooterUpdateState();
      }
      return;
    }
    if (cachedUpdateInfo.releaseUrl) {
      void invoke('open_release_page', { url: cachedUpdateInfo.releaseUrl });
      closeMenu();
      return;
    }
  }

  updateBusy = true;
  renderFooterUpdateState('检查更新中…', false);
  try {
    const info = await invoke<UpdateInfo>('check_for_update');
    updateBusy = false;
    if (info.hasUpdate) {
      cachedUpdateInfo = info;
      renderFooterUpdateState(`⬆ 点击更新 v${info.latestVersion}`, true);
    } else {
      cachedUpdateInfo = info;
      renderFooterUpdateState(`✓ 最新 v${info.currentVersion}`, false);
      setTimeout(() => {
        renderFooterUpdateState();
      }, 2200);
    }
  } catch {
    updateBusy = false;
    renderFooterUpdateState('检查失败，点此重试', false);
    setTimeout(() => {
      renderFooterUpdateState();
    }, 2200);
  }
}

async function bootstrap(): Promise<void> {
  if (isMacPlatform()) {
    const shortcutBtn = document.getElementById('btn-desktop-shortcut');
    if (shortcutBtn) shortcutBtn.classList.add('hidden');
    const lockLbl = document.getElementById('lbl-toggle-lock');
    if (lockLbl) lockLbl.textContent = '锁定鼠标穿透（Option 解锁）';
    const lockBtn = document.getElementById('btn-toggle-lock');
    if (lockBtn) lockBtn.title = '开启后重启保持锁定，按住 Option 或在菜单栏可随时解锁';
  }

  try {
    const [loadedSettings, loadedCats, ver, cachedUpd] = await Promise.all([
      invoke<AppSettings>('get_settings'),
      invoke<CatSkinConfig[]>('get_cats'),
      invoke<string>('get_app_version'),
      invoke<UpdateInfo | null>('get_cached_update_info').catch(() => null),
    ]);
    settings = loadedSettings;
    if (Array.isArray(loadedCats) && loadedCats.length > 0) {
      catList = loadedCats;
    }
    if (ver) {
      currentAppVersion = ver;
    }
    if (cachedUpd) {
      cachedUpdateInfo = cachedUpd;
    }
  } catch (err) {
    console.error('Failed to load menu state:', err);
  }
  syncUI();
  renderFooterUpdateState();
  switchTab('appearance');

  const isWin7 = await invoke<boolean>('is_win7_ulw_mode').catch(() => false);
  if (isWin7) {
    document.documentElement.classList.add('win7-ulw-mode');
  }

  document.getElementById('btn-close-menu')?.addEventListener('click', () => closeMenu());

  window.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') closeMenu();
  });

  if (scaleSliderEl) {
    scaleSliderEl.addEventListener('pointerdown', () => {
      sliderDragging = true;
    });
    scaleSliderEl.addEventListener('input', () => {
      sliderDragging = true;
      const nextScale = Math.max(0.2, Math.min(1.5, Number(scaleSliderEl.value) / 100));
      updateScaleSliderVisual(nextScale);
      void invoke('preview_scale', { scale: nextScale });
    });
    const commitSliderScale = () => {
      sliderDragging = false;
      const nextScale = Math.max(0.2, Math.min(1.5, Number(scaleSliderEl.value) / 100));
      void applySettingsPatch({ scale: nextScale });
    };
    scaleSliderEl.addEventListener('change', commitSliderScale);
    scaleSliderEl.addEventListener('pointerup', () => {
      sliderDragging = false;
    });
  }

  if (scaleValueEl) {
    scaleValueEl.addEventListener('click', (e) => {
      e.stopPropagation();
      sliderDragging = false;
      updateScaleSliderVisual(1.0);
      void applySettingsPatch({ scale: 1.0 });
    });
  }

  menuEl.addEventListener('click', (e) => {
    const tabBtn = (e.target as HTMLElement).closest<HTMLButtonElement>('[data-tab]');
    if (tabBtn) {
      const t = tabBtn.dataset.tab;
      if (t === 'appearance' || t === 'system') {
        switchTab(t);
      }
      return;
    }

    const target = (e.target as HTMLElement).closest<HTMLElement>(
      '[data-action], [data-breed], [data-rendermode], [data-collar], [data-headphonemode], [data-kblayout], [data-kborient], [data-tailmode]',
    );
    if (!target) return;

    const headphoneAttr = target.dataset.headphonemode;
    if (headphoneAttr === 'auto' || headphoneAttr === 'on' || headphoneAttr === 'off') {
      void applySettingsPatch({ headphoneMode: headphoneAttr });
      return;
    }

    const renderModeAttr = target.dataset.rendermode;
    if (renderModeAttr === '2d' || renderModeAttr === '3d') {
      void applySettingsPatch({ renderMode: renderModeAttr });
      return;
    }

    const collarAttr = target.dataset.collar;
    if (
      collarAttr === 'none' ||
      collarAttr === 'red' ||
      collarAttr === 'blue' ||
      collarAttr === 'pink' ||
      collarAttr === 'green'
    ) {
      void applySettingsPatch({ collarColor: collarAttr });
      return;
    }

    const kbLayoutAttr = target.dataset.kblayout;
    if (kbLayoutAttr === '65' || kbLayoutAttr === '108') {
      void applySettingsPatch({ keyboardLayout: kbLayoutAttr });
      return;
    }

    const kbOrientAttr = target.dataset.kborient;
    if (kbOrientAttr === 'cat' || kbOrientAttr === 'viewer') {
      void applySettingsPatch({ keyboardOrientation: kbOrientAttr });
      return;
    }

    const tailModeAttr = target.dataset.tailmode;
    if (
      tailModeAttr === 'smart' ||
      tailModeAttr === 'custom' ||
      tailModeAttr === 'edge' ||
      tailModeAttr === 'off'
    ) {
      void applySettingsPatch({ tailKeyMode: tailModeAttr });
      return;
    }

    const breedAttr = target.dataset.breed;
    if (breedAttr) {
      void applySettingsPatch({ catBreed: breedAttr });
      return;
    }

    switch (target.dataset.action) {
      case 'toggle-privacy':
        void applySettingsPatch({ privacyPasswordGuard: !(settings.privacyPasswordGuard !== false) });
        break;
      case 'toggle-bubbles':
        void applySettingsPatch({ showKeyBubbles: !settings.showKeyBubbles });
        break;
      case 'toggle-lock':
        void applySettingsPatch({ clickThroughLocked: !settings.clickThroughLocked });
        closeMenu();
        break;
      case 'toggle-sound':
        void applySettingsPatch({ soundEnabled: !settings.soundEnabled });
        break;
      case 'toggle-top':
        void applySettingsPatch({ alwaysOnTop: !settings.alwaysOnTop });
        break;
      case 'toggle-autostart':
        void applySettingsPatch({ autostart: !settings.autostart });
        break;
      case 'toggle-obs': {
        const nextObs = !settings.obsEnabled;
        void applySettingsPatch({
          obsEnabled: nextObs,
          obsHidePet: nextObs ? settings.obsHidePet : false,
        });
        break;
      }
      case 'copy-obs':
        void copyObsUrl();
        break;
      case 'toggle-obs-hide':
        void applySettingsPatch({ obsHidePet: !settings.obsHidePet });
        if (!settings.obsHidePet) {
          closeMenu();
        }
        break;
      case 'toggle-mcp':
        void applySettingsPatch({ mcpEnabled: !settings.mcpEnabled });
        break;
      case 'copy-mcp':
        void copyMcpConfig();
        break;
      case 'open-tail-picker':
        void invoke('open_tail_picker_from_menu');
        break;
      case 'desktop-shortcut':
        void createDesktopShortcut();
        break;
      case 'reset-pos':
        void invoke('reset_position');
        closeMenu();
        break;
      case 'check-update':
        void triggerManualUpdateCheck();
        break;
      case 'quit':
        void invoke('quit_app');
        break;
    }
  });

  await listen<AppSettings>('kuro-settings', (ev) => {
    settings = ev.payload;
    syncUI();
  });

  await listen<CatSkinConfig[]>('kuro-cats', (ev) => {
    if (Array.isArray(ev.payload) && ev.payload.length > 0) {
      catList = ev.payload;
      syncUI();
    }
  });

  await listen<UpdateProgressEvent>('kuro-update-status', (ev) => {
    if (ev.payload.info) {
      cachedUpdateInfo = ev.payload.info;
    }
    if (ev.payload.stage === 'available' && cachedUpdateInfo?.hasUpdate) {
      updateBusy = false;
      renderFooterUpdateState();
    } else if (ev.payload.stage === 'downloading' || ev.payload.stage === 'verifying' || ev.payload.stage === 'installing') {
      updateBusy = true;
      renderFooterUpdateState('更新中...', true);
    } else if (ev.payload.stage === 'up_to_date' || ev.payload.stage === 'error') {
      updateBusy = false;
      renderFooterUpdateState();
    }
  });

  await listen('kuro-menu-opened', async () => {
    try {
      const [latestSettings, latestCats, cachedUpd] = await Promise.all([
        invoke<AppSettings>('get_settings'),
        invoke<CatSkinConfig[]>('get_cats'),
        invoke<UpdateInfo | null>('get_cached_update_info').catch(() => null),
      ]);
      settings = latestSettings;
      if (Array.isArray(latestCats) && latestCats.length > 0) {
        catList = latestCats;
      }
      if (cachedUpd) {
        cachedUpdateInfo = cachedUpd;
      }
      syncUI();
      renderFooterUpdateState();
      if (tabBodyEl) tabBodyEl.scrollTop = 0;
    } catch {}
  });
}

void bootstrap();
