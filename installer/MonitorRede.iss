; Instalador do Monitor de Rede para Windows (Inno Setup 6).
;
; Compilado pelo GitHub Actions:
;   iscc /DAppVersion=0.5.0 /DExePath=..\target\release\monitor-rede.exe installer\MonitorRede.iss
;
; O que ele faz além de copiar o executável:
; - cria a tarefa agendada "MonitorRede" (privilégios mais altos), que o app
;   usa para abrir sempre como administrador sem a pergunta do UAC. Assim a
;   aba "Uso da rede" mostra a banda de cada aplicativo;
; - libera o app no Firewall do Windows para redes privadas (painel no
;   celular e MCP pela rede);
; - opcionalmente, inicia com o Windows (direto na bandeja).

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef ExePath
  #define ExePath "..\target\release\monitor-rede.exe"
#endif

[Setup]
AppId={{6E0D7C1B-58A4-4C4F-9F1E-2B7A3E9C4D10}
AppName=Monitor de Rede
AppVersion={#AppVersion}
AppVerName=Monitor de Rede {#AppVersion}
AppPublisher=hfabio
AppPublisherURL=https://github.com/hfabio/monitor-de-rede
DefaultDirName={autopf}\Monitor de Rede
DefaultGroupName=Monitor de Rede
DisableProgramGroupPage=yes
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\dist
OutputBaseFilename=MonitorRede-windows-x64-instalador
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayIcon={app}\MonitorRede.exe
UninstallDisplayName=Monitor de Rede

[Languages]
Name: "pt"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"

[Tasks]
Name: "autostart"; Description: "Iniciar com o Windows (abre direto na bandeja)"
Name: "desktopicon"; Description: "Criar atalho na área de trabalho"; Flags: unchecked

[Files]
Source: "{#ExePath}"; DestDir: "{app}"; DestName: "MonitorRede.exe"; Flags: ignoreversion

[Icons]
Name: "{group}\Monitor de Rede"; Filename: "{app}\MonitorRede.exe"
Name: "{autodesktop}\Monitor de Rede"; Filename: "{app}\MonitorRede.exe"; Tasks: desktopicon

[Registry]
; Mesmo valor que a opção "Iniciar com o Windows" do app grava.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "MonitorRede"; ValueData: """{app}\MonitorRede.exe"" --bandeja"; Tasks: autostart; Flags: uninsdeletevalue

[Run]
Filename: "{app}\MonitorRede.exe"; Parameters: "--instalar-tarefa"; StatusMsg: "Configurando para abrir como administrador..."; Flags: runhidden waituntilterminated
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall delete rule name=""Monitor de Rede"""; Flags: runhidden waituntilterminated
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall add rule name=""Monitor de Rede"" dir=in action=allow program=""{app}\MonitorRede.exe"" enable=yes profile=private"; StatusMsg: "Liberando no Firewall (redes privadas)..."; Flags: runhidden waituntilterminated
Filename: "{app}\MonitorRede.exe"; Description: "Abrir o Monitor de Rede"; Flags: postinstall nowait skipifsilent runasoriginaluser
; Atualização automática (instalador em modo silencioso): reabre o app.
Filename: "{app}\MonitorRede.exe"; Parameters: "{param:appargs|--bandeja}"; Flags: nowait runasoriginaluser; Check: WizardSilent

[UninstallRun]
Filename: "{sys}\taskkill.exe"; Parameters: "/f /im MonitorRede.exe"; Flags: runhidden waituntilterminated; RunOnceId: "Fechar"
Filename: "{app}\MonitorRede.exe"; Parameters: "--remover-tarefa"; Flags: runhidden waituntilterminated; RunOnceId: "Tarefa"
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall delete rule name=""Monitor de Rede"""; Flags: runhidden waituntilterminated; RunOnceId: "Firewall"

[Code]
// Fecha o app antes de atualizar (ele pode estar rodando como administrador na bandeja).
function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Code: Integer;
begin
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/f /im MonitorRede.exe', '', SW_HIDE, ewWaitUntilTerminated, Code);
  Sleep(500);
  Result := '';
end;
