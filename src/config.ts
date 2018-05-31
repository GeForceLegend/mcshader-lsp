import * as vscode from 'vscode'
import * as path from 'path'
import * as os from 'os'
import { regLinuxOutput } from './linter/glslProvider'
// glslangPath: Path to glslangValidator (assumed in PATH by default)
// workDir: the directory in which all the files should be, ending in /shaders
// tmpdir: the directory into which the symlinks are stored, should be the OS's temp dir
// isWin: are we on Windows?
export class Config {
  public readonly minecraftPath: string
  public readonly glslangPath: string
  public readonly workDir: string
  public readonly tmpdir: string
  public readonly isWin: boolean
  public readonly outputMatch: RegExp

  constructor() {
    const c = vscode.workspace.getConfiguration('mcglsl')

    console.log('[MC-GLSL] glslangValidatorPath set to', c.get('glslangValidatorPath'))
    console.log('[MC-GLSL] temp directory root set to', path.join(os.tmpdir(), vscode.workspace.name!, 'shaders'))

    this.glslangPath = c.get('glslangValidatorPath') as string
    this.minecraftPath = c.get('minecraftPath') as string
    this.isWin = os.platform() === 'win32'
    this.tmpdir = path.join(os.tmpdir(), vscode.workspace.name!, 'shaders')
    this.outputMatch = regLinuxOutput
    this.workDir = path.basename(vscode.workspace.rootPath!) === 'shaders' ?
                    vscode.workspace.rootPath! :
                    path.join(vscode.workspace.rootPath!, 'shaders')
  }

  public onChange(e: vscode.ConfigurationChangeEvent) {
    if (e.affectsConfiguration('mcglsl')) {
      console.log('[MC-GLSL] config changed')
      Object.assign(this, new Config())
    }
  }
}
