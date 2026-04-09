import * as vscode from 'vscode'
import { SurferWaveformViewerEditorProvider } from './surferWaveformViewer'
import { onSurferConfigChange, readSurferConfig } from './configTemplate'

export function activate(context: vscode.ExtensionContext) {
  console.log('Surfer has been activated!')
  const initialConfig = readSurferConfig()

  // Pass initial config to provider if supported
  context.subscriptions.push(SurferWaveformViewerEditorProvider.register(context, initialConfig))

  // React to configuration changes
  context.subscriptions.push(
    onSurferConfigChange(cfg => {
      // Broadcast or reinitialize as needed; placeholder for now
      vscode.commands.executeCommand('setContext', 'surfer.config', cfg)
    })
  )
}
