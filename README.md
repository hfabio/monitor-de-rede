# Monitor de Rede

App de desktop (Windows, macOS e Linux) que mede continuamente a latência, o jitter e a perda de pacotes até vários endereços, e mostra em linguagem simples se a internet está boa para chamadas de vídeo e jogos.

![Tela inicial](docs/1-inicio.png)

## Download

Baixe a versão mais recente em **[Releases](https://github.com/hfabio/monitor-de-rede/releases/latest)**:

| Sistema | Arquivo |
|---|---|
| Windows 10/11 (64 bits) | `MonitorRede-windows-x64.exe` |
| macOS (Apple Silicon e Intel) | `MonitorRede-macos-universal.zip` |
| Linux (x64) | `MonitorRede-linux-x64.tar.gz` |

Não precisa instalar nada.

- **Windows:** se o SmartScreen avisar "aplicativo não reconhecido", clique em **Mais informações → Executar assim mesmo**. O executável não é assinado digitalmente.
- **macOS:** descompacte, mova para Aplicativos e, na primeira vez, clique com o botão direito → **Abrir**. Isso é preciso porque o app não é assinado pela Apple.
- **Linux:** descompacte e rode `./monitor-rede`.

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

## Dados

Ficam na pasta de dados do usuário, em `MonitorRede/`:

| Sistema | Pasta |
|---|---|
| Windows | `%APPDATA%\MonitorRede` |
| macOS | `~/Library/Application Support/MonitorRede` |
| Linux | `~/.local/share/MonitorRede` |

- **`config.json`:** endereços e opções.
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

Para salvar prints de todas as telas e fechar (usado para gerar as imagens deste README):

```bash
MONITOR_REDE_SHOTS=/tmp/prints cargo run
```

## Releases

O workflow [`.github/workflows/build.yml`](.github/workflows/build.yml) compila e testa nos três sistemas a cada push. Ao enviar uma tag `v*`, ele também publica os executáveis numa Release:

```bash
git tag v0.2.0 && git push origin v0.2.0
```

## Estrutura

| Arquivo | Conteúdo |
|---|---|
| `src/probe.rs` | ping ICMP (API nativa no Windows, sem administrador), conexão TCP e detecção do roteador padrão |
| `src/monitor.rs` | uma thread por endereço, com estatísticas e eventos |
| `src/history.rs` | histórico em SQLite |
| `src/netusage.rs` | banda total e banda por aplicativo (Windows) |
| `src/autostart.rs` | iniciar com o sistema |
| `src/ui/` | telas, feitas com [egui](https://github.com/emilk/egui) |
