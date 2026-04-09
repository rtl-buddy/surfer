import * as vscode from 'vscode'

export interface SurferLayoutLike {
  showHierarchy: boolean
  showMenu: boolean
  showToolbar: boolean
  showTicks: boolean
  showTooltip: boolean
  showScopeTooltip: boolean
  showOverview: boolean
  showStatusbar: boolean
  showVariableIndices: boolean
  showVariableDirection: boolean
  showDefaultTimeline: boolean
  showEmptyScopes: boolean
  parameterDisplayLocation: string
  windowHeight: number
  windowWidth: number
  alignNamesRight: boolean
  hierarchyStyle: string
  waveformsTextSize: number
  waveformsLineHeight: number
  waveformsLineHeightMultiples: number[]
  transactionsLineHeight: number
  zoomFactors: number[]
  defaultZoomFactor: number
  highlightFocused: boolean
  moveFocusOnInsertedMarker: boolean
  fillHighValues: boolean
  useDinotraceStyle: boolean
}

export interface SurferConfigLike {
  theme: string
  layout: SurferLayoutLike
  snapDistance: number
  undoStackSize: number
  animation: {
    enabled: boolean
    time: number
  }
  maxUrlLength: number
  behavior: {
    keepDuringReload: boolean
    arrowKeyBindings: 'Edge' | 'Scroll'
    primaryButtonDragBehavior: 'Measure' | 'Cursor'
  }
  gesture: {
    size: number
    deadzone: number
    backgroundRadius: number
    backgroundGamma: number
  }
}

export function readSurferConfig(): SurferConfigLike {
  const cfg = vscode.workspace.getConfiguration('surfer')
  return {
    theme: cfg.get<string>('theme', 'dark+'),
    layout: {
      showHierarchy: cfg.get<boolean>('layout.showHierarchy', true),
      showMenu: cfg.get<boolean>('layout.showMenu', true),
      showToolbar: cfg.get<boolean>('layout.showToolbar', true),
      showTicks: cfg.get<boolean>('layout.showTicks', true),
      showTooltip: cfg.get<boolean>('layout.showTooltip', true),
      showScopeTooltip: cfg.get<boolean>('layout.showScopeTooltip', true),
      showOverview: cfg.get<boolean>('layout.showOverview', true),
      showStatusbar: cfg.get<boolean>('layout.showStatusbar', true),
      showVariableIndices: cfg.get<boolean>('layout.showVariableIndices', false),
      showVariableDirection: cfg.get<boolean>('layout.showVariableDirection', true),
      showDefaultTimeline: cfg.get<boolean>('layout.showDefaultTimeline', true),
      showEmptyScopes: cfg.get<boolean>('layout.showEmptyScopes', true),
      parameterDisplayLocation: cfg.get<string>('layout.parameterDisplayLocation', 'Top'),
      windowHeight: cfg.get<number>('layout.windowHeight', 800),
      windowWidth: cfg.get<number>('layout.windowWidth', 1200),
      alignNamesRight: cfg.get<boolean>('layout.alignNamesRight', false),
      hierarchyStyle: cfg.get<string>('layout.hierarchyStyle', 'Separate'),
      waveformsTextSize: cfg.get<number>('layout.waveformsTextSize', 13),
      waveformsLineHeight: cfg.get<number>('layout.waveformsLineHeight', 18),
      waveformsLineHeightMultiples: cfg.get<number[]>('layout.waveformsLineHeightMultiples', [1, 1.5, 2]),
      transactionsLineHeight: cfg.get<number>('layout.transactionsLineHeight', 18),
      zoomFactors: cfg.get<number[]>('layout.zoomFactors', [0.9, 1.0, 1.1]),
      defaultZoomFactor: cfg.get<number>('layout.defaultZoomFactor', 1.0),
      highlightFocused: cfg.get<boolean>('layout.highlightFocused', true),
      moveFocusOnInsertedMarker: cfg.get<boolean>('layout.moveFocusOnInsertedMarker', false),
      fillHighValues: cfg.get<boolean>('layout.fillHighValues', true),
      useDinotraceStyle: cfg.get<boolean>('layout.useDinotraceStyle', false)
    },
    snapDistance: cfg.get<number>('snapDistance', 6),
    undoStackSize: cfg.get<number>('undoStackSize', 100),
    animation: {
      enabled: cfg.get<boolean>('animation.enabled', true),
      time: cfg.get<number>('animation.time', 0.12)
    },
    maxUrlLength: cfg.get<number>('maxUrlLength', 4000),
    behavior: {
      keepDuringReload: cfg.get<boolean>('behavior.keepDuringReload', true),
      arrowKeyBindings: cfg.get<'Edge' | 'Scroll'>('behavior.arrowKeyBindings', 'Edge'),
      primaryButtonDragBehavior: cfg.get<'Measure' | 'Cursor'>('behavior.primaryButtonDragBehavior', 'Measure')
    },
    gesture: {
      size: cfg.get<number>('gesture.size', 160),
      deadzone: cfg.get<number>('gesture.deadzone', 400),
      backgroundRadius: cfg.get<number>('gesture.backgroundRadius', 0.85),
      backgroundGamma: cfg.get<number>('gesture.backgroundGamma', 0.6)
    }
  }
}

export function onSurferConfigChange(listener: (cfg: SurferConfigLike) => void): vscode.Disposable {
  const disp = vscode.workspace.onDidChangeConfiguration(e => {
    if (e.affectsConfiguration('surfer')) {
      listener(readSurferConfig())
    }
  })
  return disp
}
