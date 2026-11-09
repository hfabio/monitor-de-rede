# Monitor de Rede

App de desktop (Windows, macOS e Linux), com uma versão simplificada para Android, que mede continuamente a latência, o jitter e a perda de pacotes até vários endereços, e mostra em linguagem simples se a internet está boa para chamadas de vídeo e jogos.

![Tela inicial](docs/1-inicio.png)

## Download

Baixe a versão mais recente em **[Releases](https://github.com/hfabio/monitor-de-rede/releases/latest)**:

| Sistema | Arquivo |
|---|---|
| Windows 10/11 (64 bits), instalador (recomendado) | `MonitorRede-windows-x64-instalador.exe` |
| Windows 10/11 (64 bits), sem instalar | `MonitorRede-windows-x64.exe` |
| macOS (Apple Silicon e Intel) | `MonitorRede-macos.dmg` |
| Linux (x64) | `MonitorRede-linux-x64.tar.gz` |
| Android 8 ou mais novo (64 bits) | `MonitorRede-android-arm64.apk` |

No Mac e no Linux, não precisa instalar nada. No Android, veja [Android](#android).

- **Windows (instalador):** instala em Arquivos de Programas e deixa o app abrindo sempre como administrador, sem a pergunta do Windows (UAC). Assim a aba **Uso da rede** mostra a banda de cada aplicativo.
  - Para isso, cria a tarefa agendada **MonitorRede**, com privilégios mais altos. O atalho e o "Iniciar com o Windows" disparam essa tarefa.
  - Também libera o app no Firewall para redes privadas, para o painel no celular e o MCP pela rede funcionarem sem aviso.
  - Pede a senha de administrador uma vez, na instalação. Se quem usa o PC não for administrador, a tarefa fica na conta de quem digitou a senha; nesse caso use o `.exe` sem instalar.
  - Para desinstalar: **Configurações → Aplicativos**. A desinstalação remove a tarefa e a regra do Firewall; os dados em `%APPDATA%\MonitorRede` ficam.
- **Windows (sem instalar):** o `.exe` roda direto. Para ver a banda por aplicativo, use o botão **Reabrir como administrador**.
- **Windows:** se o SmartScreen avisar "aplicativo não reconhecido", clique em **Mais informações → Executar assim mesmo**. O app não é assinado digitalmente.
- **macOS:** abra o `.dmg` e arraste o app para **Aplicativos**. Na primeira vez, clique com o botão direito → **Abrir**, porque o app não é assinado pela Apple. Se você abrir o app direto da imagem ou dos Downloads, ele mesmo se copia para Aplicativos (ou `~/Applications`, sem permissão) e abre de lá.
- **Linux:** descompacte e rode `./monitor-rede`.

## Atualização automática

O app procura versões novas nas [Releases](https://github.com/hfabio/monitor-de-rede/releases) ao abrir e a cada 6 horas. Com **Atualizar automaticamente** ligado (padrão, em **Configurações**), ele baixa, instala e reabre sozinho, voltando para a bandeja se estava lá. Com a opção desligada, aparece o aviso **Atualizar para a versão X** no topo da janela.

- **Windows instalado:** roda o instalador novo em modo silencioso.
- **Windows sem instalar e Linux:** troca o executável no mesmo lugar.
- **macOS:** troca o `Monitor de Rede.app` (o `.zip` da Release).
- **Android:** o Android não deixa um app se atualizar sozinho fora da Play Store. O app avisa na tela Início e em Ajustes, e o botão **Baixar atualização** abre o download do APK novo.

## Bandeja do sistema

No Windows e no macOS, **fechar a janela não encerra o app**: ele vai para a bandeja do sistema (perto do relógio) e continua monitorando.

- **Cor do ícone:** acompanha a rede (verde, amarelo ou vermelho). Passe o mouse para ver o resumo.
- **Clique no ícone** (ou **Abrir Monitor de Rede**): mostra a janela de novo.
- **Botão direito → Encerrar app:** fecha de verdade.
- **Iniciar com o sistema:** com essa opção ligada, o app abre direto na bandeja, sem mostrar a janela (argumento `--bandeja`).

No Linux, a bandeja exigiria GTK. Lá, fechar a janela encerra o app.

## Telas

**Início:** mostra o status geral em português do dia a dia e um cartão por endereço, com a latência.
- **Verde:** até 80 ms.
- **Amarelo:** de 80 ms a 1 s.
- **Vermelho:** acima de 1 s, sem resposta ou mais de 5% de perda.

**Monitor:** um diagnóstico automático que separa problema na rede de casa de problema na operadora. Tem gráficos de latência, jitter e perda (janelas de 1 min, 5 min, 15 min e 1 h) e uma tabela com último valor, média, mínimo/máximo, jitter e perda.

![Monitor](docs/2-monitor-latencia.png)

**Uso da rede:** mostra a banda total de download e upload e o gráfico dos últimos 5 minutos.
- No Windows, também mostra a banda e as conexões de cada aplicativo.
- A banda por aplicativo exige abrir o app como administrador. Sem isso, aparecem só as conexões, e há um botão para reabrir como administrador.
- Abas do Chrome aparecem juntas numa linha só.
- Tráfego UDP entra só no total.

**Eventos:** registra quedas (3 ou mais medições seguidas sem resposta), retornos, picos de latência, perda de pacotes acima do limite em 1 minuto e erros de DNS. Os filtros mostram a quantidade de cada tipo nas últimas 24 h, e dá para exportar em CSV.

![Eventos](docs/6-eventos.png)

**Configurações:** dá para adicionar, renomear, desativar e remover endereços.
- Há sugestões prontas de jogos (servidores BR) e de apps (Teams, Zoom, WhatsApp, Netflix, YouTube, Instagram e TikTok).
- Também se ajustam o intervalo, o timeout, o limite de pico e o limite de perda.
- O histórico fica em SQLite, com retenção configurável em dias.
- Tem a opção de iniciar junto com o sistema.

![Configurações](docs/7-configuracoes.png)

### Formatos de endereço

| Exemplo | Como é medido |
|---|---|
| `8.8.8.8`, `google.com` | ping (ICMP) |
| `gateway` | ping no roteador padrão, detectado automaticamente |
| `https://site.com/caminho` | tempo de conexão TCP na porta 443, para servidores que bloqueiam ping |
| `http://site.com`, `1.2.3.4:27015` | tempo de conexão TCP na porta indicada |

As sugestões de jogos usam `https://dynamodb.sa-east-1.amazonaws.com/ping` (AWS São Paulo). Esse endereço dá uma boa aproximação da latência até os servidores brasileiros.

## Painel no celular

Com o app aberto no PC, dá para acompanhar a rede pelo navegador do celular (Android ou iPhone), no mesmo Wi-Fi. A página mostra o status geral, um cartão por endereço com a latência dos últimos minutos, a banda atual e os últimos eventos, e se atualiza a cada 2 segundos.

- A aba **Configurações** mostra o endereço para abrir no celular, como `http://192.168.68.11:47810`, com um botão para copiar.
- Dá para desligar o painel ou trocar a porta (padrão 47810; até a versão 0.6.0 era 8080, e configurações antigas mudam sozinhas) no cartão **Painel no celular**.
- É só leitura: pelo celular não dá para mudar a configuração nem rodar testes.
- **Windows:** na primeira vez, o Firewall pode pedir permissão. Clique em **Permitir acesso** para redes privadas.
- Se a página mostrar "Sem conexão com o PC", confira se o app está aberto e se o celular está no mesmo Wi-Fi.

## Android

O APK é uma versão simplificada, boa para andar pela casa com o celular e ver onde o Wi-Fi fica lento (por exemplo, perto de cada Deco).

**Instalar:** baixe o `MonitorRede-android-arm64.apk` no celular e abra o arquivo. O Android vai pedir para **permitir instalar apps desconhecidos** para o navegador ou o gerenciador de arquivos. Permita e toque em **Instalar**. O app não está na Play Store.

| Início | Este celular | PC da casa | Ajustes |
|---|---|---|---|
| <img src="docs/android-1-home.png" width="200" alt="Início"> | <img src="docs/android-2-phone.png" width="200" alt="Este celular"> | <img src="docs/android-3-pc.png" width="200" alt="PC da casa"> | <img src="docs/android-4-settings.png" width="200" alt="Ajustes"> |

- **Mede só com o app aberto.** Não roda em segundo plano nem manda notificações. Ao sair (botão Voltar na tela inicial), as medições param.
- **Início:** veredito da casa (este celular e o PC), dois cartões com a latência de cada um e os últimos avisos dos dois.
- **Este celular:** gráfico dos últimos 5 minutos e um cartão por endereço, com as mesmas cores do desktop. O botão **+ Adicionar endereço** abre um formulário com sugestões; o **×** remove.
- **PC da casa:** mostra o que o app do PC está vendo, lido do [Painel no celular](#painel-no-celular) a cada 2 segundos: status, endereços e uso de rede do PC. Só funciona no mesmo Wi-Fi.
- **Ajustes:** **Procurar PCs** acha os PCs com o painel ligado nesta rede (ou digite o IP). Também ajusta o intervalo, o tempo de espera e a porta do painel.
- O **Roteador** é detectado pelo Android. Se não der, numa rede de Deco é usado `192.168.68.1`.

Não tem no Android: uso da rede por aplicativo, banda total, iniciar com o sistema, bandeja, MCP e exportar CSV. O APK é assinado com uma chave de teste. Se uma versão nova não instalar por cima da antiga, desinstale a antiga antes.

## Integração com IA (MCP)

O app também funciona como servidor [MCP](https://modelcontextprotocol.io). Assim, assistentes como Claude, Codex, Antigravity e Cursor podem consultar a rede, e você pode perguntar coisas como "minha internet caiu hoje de manhã?" ou "o servidor do jogo está com perda agora?".

Dá para usar de dois jeitos. A aba **Configurações** mostra os trechos prontos dos dois, com um botão para copiar.

### Neste computador

O assistente roda o executável com o argumento `mcp`, que inicia o servidor por stdio, sem janela.

**Claude Code**

```bash
claude mcp add monitor-rede -- "C:\caminho\MonitorRede.exe" mcp
```

**Claude Desktop, Antigravity, Cursor** (seção `mcpServers` do arquivo de configuração):

```json
{
  "mcpServers": {
    "monitor-rede": { "command": "C:\\caminho\\MonitorRede.exe", "args": ["mcp"] }
  }
}
```

**Codex** (`~/.codex/config.toml`):

```toml
[mcp_servers.monitor-rede]
command = 'C:\caminho\MonitorRede.exe'
args = ["mcp"]
```

No macOS, o comando fica `"/Applications/Monitor de Rede.app/Contents/MacOS/monitor-rede"`.

### De outro computador da rede

Com o app aberto (também na bandeja), ele atende MCP por HTTP em `http://<IP do PC>:47810/mcp`, no mesmo servidor do [painel no celular](#painel-no-celular). Assim, do seu notebook você acompanha o seu PC, o de outra pessoa da casa e a rede de todos ao mesmo tempo: cadastre um servidor por computador.

- O acesso exige um token, gerado na primeira vez e mostrado em **Configurações → Integração com IA → De outro computador da rede**. Lá também dá para desligar o acesso pela rede ou gerar um token novo.
- Cada servidor se apresenta com o nome do computador (por exemplo `monitor-rede-pc-da-ana`), e o `get_status` traz o campo `computer`, para o assistente não misturar os PCs.
- Funciona só na rede de casa: o servidor não usa HTTPS, então não exponha a porta para a internet.

**Claude Code**

```bash
claude mcp add --transport http monitor-rede-pc-da-ana http://192.168.68.11:47810/mcp --header "Authorization: Bearer <token>"
```

**Cursor, Antigravity e outros** (`mcpServers`):

```json
{
  "mcpServers": {
    "monitor-rede-pc-da-ana": {
      "type": "http",
      "url": "http://192.168.68.11:47810/mcp",
      "headers": { "Authorization": "Bearer <token>" }
    }
  }
}
```

**Codex** (`~/.codex/config.toml`):

```toml
[mcp_servers.monitor-rede-pc-da-ana]
url = "http://192.168.68.11:47810/mcp"
http_headers = { "Authorization" = "Bearer <token>" }
```

**Claude Desktop**

O Claude Desktop só abre servidores locais pelo arquivo de configuração. Para um PC da rede, use a ponte [`mcp-remote`](https://www.npmjs.com/package/mcp-remote), que precisa do [Node.js](https://nodejs.org) instalado.

1. No Claude Desktop, abra **Configurações → Desenvolvedor → Editar configuração**. O arquivo é `claude_desktop_config.json`: no macOS, em `~/Library/Application Support/Claude/`; no Windows, em `%APPDATA%\Claude\`.
2. Acrescente um servidor por PC em `mcpServers`, com o IP e o token de cada um:

```json
{
  "mcpServers": {
    "monitor-rede-pc-da-ana": {
      "command": "npx",
      "args": [
        "-y", "mcp-remote", "http://192.168.68.11:47810/mcp",
        "--allow-http",
        "--header", "Authorization:${AUTH}"
      ],
      "env": { "AUTH": "Bearer <token>" }
    }
  }
}
```

3. Feche o Claude Desktop por completo (inclusive o ícone na barra de menus ou na bandeja) e abra de novo. Os servidores aparecem no menu de ferramentas da conversa.

- O `--allow-http` é preciso porque o servidor não usa HTTPS; ele só funciona na rede de casa.
- O token vai na variável `AUTH` porque o Claude Desktop no Windows quebra argumentos com espaço.
- Os conectores personalizados (**Configurações → Conectores**) não servem aqui: eles são acessados pelos servidores da Anthropic, que não enxergam a rede da sua casa.

### Achar os computadores na rede

Cada PC com o app aberto responde em `http://<IP>:47810/api/status` sem token, com o nome do computador no campo `computer`. Os comandos abaixo testam todos os endereços da sua rede e listam os que responderam, já com o endereço do MCP. O token não aparece na busca: pegue o de cada PC na tela **Configurações** dele.

**macOS e Linux** (no terminal):

```bash
PORTA=47810
REDE=$( (ipconfig getifaddr en0 2>/dev/null || hostname -I | cut -d' ' -f1) | cut -d. -f1-3)
for i in $(seq 1 254); do
  (nome=$(curl -s -m 1 "http://$REDE.$i:$PORTA/api/status" | grep -o '"computer":"[^"]*"' | cut -d'"' -f4)
   [ -n "$nome" ] && echo "$REDE.$i  $nome  http://$REDE.$i:$PORTA/mcp") &
done; wait
```

**Windows** (no PowerShell):

```powershell
$porta = 47810
$ip = (Get-NetIPConfiguration | Where-Object { $_.IPv4DefaultGateway } | Select-Object -First 1).IPv4Address.IPAddress
$rede = $ip -replace '\.\d+$', ''
Add-Type -AssemblyName System.Net.Http
$http = New-Object System.Net.Http.HttpClient
$http.Timeout = [TimeSpan]::FromSeconds(2)
$testes = 1..254 | ForEach-Object { [pscustomobject]@{ ip = "$rede.$_"; r = $http.GetStringAsync("http://$rede.$_:$porta/api/status") } }
foreach ($t in $testes) {
  try { $s = $t.r.GetAwaiter().GetResult() | ConvertFrom-Json; "$($t.ip)  $($s.computer)  http://$($t.ip):$porta/mcp" } catch {}
}
```

Exemplo de resultado:

```text
192.168.68.11  LUANA-PC  http://192.168.68.11:47810/mcp
192.168.68.14  MacBook Pro do Helton  http://192.168.68.14:47810/mcp
```

- Os comandos varrem só a sua faixa (`192.168.68.1` a `.254`, por exemplo). Na rede dos Decos da TP-Link, que vai de `192.168.68.x` a `192.168.71.x`, um PC numa faixa diferente da sua não aparece: troque `REDE` (ou `$rede`) por `192.168.69`, `192.168.70` ou `192.168.71` e rode de novo.
- Se um PC não aparecer, confira se o app está aberto nele, se o **Painel no celular** está ligado e, no Windows, se o Firewall liberou o app. O instalador já cria essa regra.
- O app Android faz a mesma busca em **Ajustes → Procurar PCs**.

### Ferramentas

| Ferramenta | O que faz |
|---|---|
| `get_status` | situação atual: veredito em linguagem simples, latência/jitter/perda de cada endereço no último minuto e nos últimos 5 minutos, banda atual, aplicativos que mais usam a rede e eventos recentes |
| `get_events` | quedas, retornos, picos, perdas e erros de DNS de um período, com filtros por tipo e por endereço |
| `get_history` | resumo de um período por endereço (perda, média, melhor e pior latência, jitter), os piores minutos e uma série de até 60 pontos |
| `probe_host` | testa um endereço na hora (ping, `gateway`, `https://…` ou `host:porta`) |
| `list_targets` | endereços e opções configurados |

O status ao vivo vem do app aberto, que grava `status.json` a cada 2 segundos. O histórico, os eventos e o `probe_host` funcionam mesmo com o app fechado.

## Dados

Ficam na pasta de dados do usuário, em `MonitorRede/`:

| Sistema | Pasta |
|---|---|
| Windows | `%APPDATA%\MonitorRede` |
| macOS | `~/Library/Application Support/MonitorRede` |
| Linux | `~/.local/share/MonitorRede` |
| Android | pasta interna do app (é apagada ao desinstalar) |

- **`config.json`:** endereços e opções.
- **`status.json`:** status ao vivo, usado pelo MCP.
- **`celular.json`** (só no Android): o PC da casa escolhido.
- **`historico.sqlite`:** tabela `minutos`, com um resumo por minuto de cada endereço (enviados, perdidos, mínimo/média/máximo, jitter e picos), e tabela `eventos`.

## Compilar

Pré-requisito: Rust estável ([rustup.rs](https://rustup.rs)).

```bash
cargo run --release
```

No Linux, instale antes as dependências gráficas:

```bash
sudo apt-get install libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libssl-dev
```

Para gerar o `.exe` do Windows a partir do macOS:

```bash
brew install rustup mingw-w64
rustup toolchain install stable && rustup target add x86_64-pc-windows-gnu
CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc \
  cargo +stable build --release --target x86_64-pc-windows-gnu
```

Para gerar o APK Android (precisa do Android SDK com build-tools, do NDK e de Java):

```bash
rustup target add aarch64-linux-android
ANDROID_HOME=~/Library/Android/sdk ANDROID_NDK_HOME=~/Library/Android/sdk/ndk/<versão> \
  android/build-apk.sh
```

O APK sai em `target/android/MonitorRede-android-arm64.apk`. Não usa Gradle: o script compila a biblioteca com o NDK e monta o pacote com `aapt2`, `zipalign` e `apksigner`. Sem `KEYSTORE` e `KEYSTORE_PASS`, ele cria uma chave de teste.

Para salvar prints de todas as telas e fechar (usado para gerar as imagens deste README):

```bash
MONITOR_REDE_SHOTS=/tmp/prints cargo run
```

## Releases

O workflow [`.github/workflows/build.yml`](.github/workflows/build.yml) compila e testa nos três sistemas e gera o APK Android a cada push. Ao enviar uma tag `v*`, ele também publica os executáveis e o APK numa Release.

Por padrão, cada build assina o APK com uma chave de teste nova. Para manter a mesma chave entre versões (e atualizar sem desinstalar), cadastre os secrets `ANDROID_KEYSTORE_BASE64` (o arquivo `.keystore` em base64) e `ANDROID_KEYSTORE_PASSWORD`.

```bash
git tag v0.6.1 && git push origin v0.6.1
```

## Estrutura

| Arquivo | Conteúdo |
|---|---|
| `src/probe.rs` | ping ICMP (API nativa no Windows e socket ICMP no Android, sem administrador), conexão TCP e detecção do roteador padrão |
| `src/monitor.rs` | uma thread por endereço, com estatísticas e eventos |
| `src/history.rs` | histórico em SQLite |
| `src/netusage.rs` | banda total e banda por aplicativo (Windows) |
| `src/autostart.rs` | iniciar com o sistema |
| `src/assess.rs` | avaliação em linguagem simples (verde, amarelo, vermelho) |
| `src/status.rs` | status ao vivo em JSON |
| `src/mcp.rs` | servidor MCP (stdio; pela rede, via `src/web.rs`) |
| `src/update.rs` | atualização automática pelas Releases do GitHub e, no macOS, cópia para Aplicativos |
| `src/instance.rs` | uma cópia só do app; no Windows instalado, abre como administrador pela tarefa agendada |
| `installer/MonitorRede.iss` | instalador do Windows (Inno Setup) |
| `src/tray.rs` | ícone na bandeja do sistema |
| `src/web.rs` | painel no celular (servidor HTTP e a página `src/web.html`) e MCP pela rede (`POST /mcp`) |
| `src/ui/` | telas do desktop, feitas com [egui](https://github.com/emilk/egui) |
| `src/lib.rs` | módulos compartilhados entre o desktop (`src/main.rs`) e o Android |
| `src/mobile/` | telas do Android e conexão com o PC da casa (`pc.rs`: leitura do painel e busca na rede) |
| `src/android.rs` | entrada do Android (NativeActivity), pasta de dados, roteador padrão e barras do sistema |
| `android/` | `AndroidManifest.xml`, ícone e `build-apk.sh`, que gera o APK |
