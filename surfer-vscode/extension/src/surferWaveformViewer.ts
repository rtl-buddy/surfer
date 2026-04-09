import * as vscode from 'vscode'
import { SurferConfigLike } from './configTemplate'

class Wavefile implements vscode.CustomDocument {
  uri: vscode.Uri
  constructor(uri: vscode.Uri) {
    this.uri = uri
  }
  dispose(): void {}
}

export class SurferWaveformViewerEditorProvider
  implements vscode.CustomReadonlyEditorProvider<Wavefile>
{
  public static register(context: vscode.ExtensionContext, cfg?: SurferConfigLike): vscode.Disposable {
    const provider = new SurferWaveformViewerEditorProvider(context, cfg)
    const providerRegistration = vscode.window.registerCustomEditorProvider(
      SurferWaveformViewerEditorProvider.viewType,
      provider,
      {
        webviewOptions: {
          retainContextWhenHidden: true,
        },
      }
    )

    return providerRegistration
  }
  private static readonly viewType = 'surfer.waveformViewer'

  constructor(private readonly context: vscode.ExtensionContext, private readonly cfg?: SurferConfigLike) {}
  async openCustomDocument(
    uri: vscode.Uri,
    _openContext: vscode.CustomDocumentOpenContext,
    _token: vscode.CancellationToken
  ): Promise<Wavefile> {
    return new Wavefile(uri)
  }
  async resolveCustomEditor(
    document: vscode.CustomDocument,
    webviewPanel: vscode.WebviewPanel,
    _token: vscode.CancellationToken
  ): Promise<void> {
    // Add the folder that the document lives in as a localResourceRoot
    const uriString = document.uri.toString()
    // TODO: this probably doesn't play well with windows
    const dirPath = uriString.substring(0, uriString.lastIndexOf('/'))

    webviewPanel.webview.options = {
      enableScripts: true,
      localResourceRoots: [
        vscode.Uri.joinPath(this.context.extensionUri, 'surfer'),
        vscode.Uri.parse(dirPath, true),
      ],
    }

    webviewPanel.webview.html = await this.getHtmlForWebview(webviewPanel.webview)

    const documentUri = webviewPanel.webview.asWebviewUri(document.uri).toString()

    webviewPanel.webview.onDidReceiveMessage((message) => {
      switch (message.command) {
        case 'loaded': {
          console.log("Surfer got the loaded message from the web view")
          // Send initial configuration followed by the URL to load
          if (this.cfg) {
            webviewPanel.webview.postMessage({ command: 'SetConfig', config: this.cfg })
          }
          webviewPanel.webview.postMessage({command: "LoadUrl", url: documentUri})
          break;
        }

        default: {
          console.log(`Got unexpected command (${message.command}) from surfer web view`)
        }
      }
    })
  }

  // Get the static html used for the editor webviews.
  private async getHtmlForWebview(webview: vscode.Webview): Promise<string> {
    // Read index.html from disk
    const indexPath = vscode.Uri.joinPath(this.context.extensionUri, 'surfer', 'index.html')
    const contents = await vscode.workspace.fs.readFile(indexPath)

    // Replace local paths with paths derived from WebView URIs
    let html = new TextDecoder().decode(contents)

    const translate_url = (path: string[], in_html: string) => {
      const uriString = webview
        .asWebviewUri(vscode.Uri.joinPath(this.context.extensionUri, ...path))
        .toString()
      html = this.replaceAll(html, `["']${in_html}['"]`, `"${uriString}"`)
    }

    translate_url(["surfer", "manifest.json"], "/manifest.json");
    translate_url(["surfer", "surfer_bg.wasm"], "/surfer_bg.wasm");
    translate_url(["surfer", "surfer.js"], "/surfer.js");
    translate_url(["surfer", "sw.js"], "/sw.js");
    translate_url(["surfer", "integration.js"], "integration.js");

    // We need to wait until the new HTML has loaded before we can start injecting messages.
    // To detect this, we'll add a script at the every end of the html that sends a message
    // back to the host saying that the HTML is loaded.
    // A hacky but working way of doing this is to replace the </body> tag with a script
    // and a new </body> tag
    const load_notifier = `
        (function() {
            const vscode = acquireVsCodeApi();

            vscode.postMessage({
                command: 'loaded',
            })
        }())`
    html = html.replaceAll("/*SURFER_SETUP_HOOKS*/", `${load_notifier}`)

    return html
  }

  private replaceAll(input: string, search: string, replacement: string): string {
    return input.replace(new RegExp(search, 'g'), replacement)
  }
}
