# bike-osm-ride — em português

> **Tradução.** A versão autoritativa deste documento é o [`README.md`](../../../README.md) húngaro.
> As capturas de ecrã mostram a interface original (húngara); a aplicação fala húngaro, inglês, espanhol e português.
> O manual e a política de privacidade também estão disponíveis em português: [guia do utilizador](USER_GUIDE.md) ·
> [política de privacidade](PRIVACY.md).

**Idioma:** [Húngaro](../../../README.md) · [English](../en/README.md) · [Español](../es/README.md) · Português

<p align="center">
  <img src="../../../packaging/icons/128.png" width="96" alt="ícone do bike-osm-ride">
</p>

<p align="center">
  <b>Pedalar virtualmente nos teus próprios percursos GPX, com um rolo inteligente – ou sem ele.</b><br>
  Mapa, perfil de altitude, treinos, fantasma, exportação TCX. Rápido, moderno, 2D. Rust.
</p>

<p align="center">
  <a href="https://github.com/pirelike/bike-osm-ride/releases/latest"><img alt="Última versão" src="https://img.shields.io/github/v/release/pirelike/bike-osm-ride?display_name=tag&sort=semver"></a>
  <img alt="Licença: MIT ou Apache-2.0" src="https://img.shields.io/badge/licence-MIT%20OR%20Apache--2.0-blue">
  <img alt="Plataformas" src="https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20macOS%20%7C%20Android-informational">
  <img alt="Rust" src="https://img.shields.io/badge/Rust-2021-orange?logo=rust&logoColor=white">
  <img alt="Interface: húngaro, inglês, espanhol e português" src="https://img.shields.io/badge/interface-H%C3%BAngaro%20%7C%20Ingl%C3%AAs%20%7C%20Espanhol%20%7C%20Portugu%C3%AAs-brightgreen">
</p>

<p align="center">
  <img src="../../screenshots/ride-dark.png" alt="O ecrã de pedalada no tema escuro: mapa, potência, perfil de altitude, estatísticas" width="100%">
</p>

**Salta para:** [Transferência](#transferência) · [Início rápido](#início-rápido) · [O que faz](#o-que-faz) · [Capturas de ecrã](#capturas-de-ecrã) ·
[Dispositivos e estado](#dispositivos-e-estado) · [Documentação](#documentação) · [Desenvolvimento](#desenvolvimento) · [Licença](#licença)

## Porquê este?

- **O teu percurso.** Não é um mundo pré-construído: podes pedalar qualquer GPX ou link do bikerouter, com elevação e subidas reais,
  e o rolo segue o declive do percurso.
- **Todos os números que importam.** Potência (média de 3 s, W/kg, zona), cadência, frequência cardíaca, velocidade, declive,
  NP/IF/TSS, desnível acumulado, hora prevista de chegada: em números grandes e legíveis, sem 3D.
- **Treinos e análise.** 93 treinos incluídos, com fontes, em modo ERG, com mensagens do treinador; fantasma e segmentos locais
  com PR; exportação TCX.
- **Continua a ser teu.** Sem conta, sem telemetria, os teus dados ficam na tua máquina ([privacidade](PRIVACY.md)). Código
  aberto, com uma licença permissiva.
- **Funciona sem hardware.** Com o ciclista simulado podes experimentar tudo sem Bluetooth.

## Transferência

A versão mais recente: **[Releases → Latest](https://github.com/pirelike/bike-osm-ride/releases/latest)**.
Nos nomes dos ficheiros, `<version>` é o número da versão (por exemplo `0.3.0`).

| Plataforma | Ficheiro | Instalação |
|---|---|---|
| **Windows 10/11** | `bike-osm-ride-<version>-windows-x64.exe` | sem instalador: executa-o. Precisa do [runtime WebView2](https://developer.microsoft.com/microsoft-edge/webview2/) (o Windows 11 já o tem por omissão). O programa não está assinado digitalmente, por isso o SmartScreen pode bloqueá-lo. |
| **Linux** (x86-64, glibc 2.35+) | `bike-osm-ride-<version>-linux-x86_64.AppImage` | `chmod +x` e executa-o; traz o WebKitGTK e o GTK consigo |
| | `bike-osm-ride-<version>-linux-amd64.deb` | `sudo apt install ./bike-osm-ride-<version>-linux-amd64.deb` (pede os pacotes `libwebkit2gtk-4.1-0`, `libgtk-3-0` e `bluez`) |
| **macOS 11+** (Intel e Apple Silicon) | `bike-osm-ride-<version>-macos-universal.dmg` | arrasta-o para *Aplicações*; no primeiro arranque, **clique com o botão direito → Abrir → Abrir** (não está notarizado); na primeira pesquisa Bluetooth, permite o Bluetooth |
| **Android 7+** | `bike-osm-ride-<version>-android.apk` | copia-o para o telemóvel e abre-o (tens de permitir a instalação a partir de origens desconhecidas), ou `adb install -r` |

O programa **avisa-te** quando há uma versão nova: o botão *Atualização* aparece na barra superior (no Windows e
como AppImage substitui-se a si próprio; nas outras plataformas abre a página da versão). O pacote para Android está assinado
com uma chave de depuração, por isso, entre duas versões, pode ser preciso desinstalar a anterior.

## Início rápido

1. **Arranca.** Abre-se a janela *Percurso*: arrasta um GPX para lá, cola um link do bikerouter ou escolhe o
   *Percurso de demonstração*.
2. **Liga o rolo** (*Dispositivos* → *Iniciar pesquisa* → na função *Rolo*), ou escolhe o
   **ciclista simulado** se não tiveres hardware à mão. Também podes ligar um monitor de frequência cardíaca,
   um potenciómetro, um sensor de cadência e um Zwift Click/Play.
3. *(Opcional)* **Treino**: com o botão *Treino* da barra superior (`W`), escolhe um dos 93 treinos ou importa o teu.
4. **`Iniciar`** (`Espaço`). O rolo segue as subidas (ou mantém os watts alvo do treino) e os números ganham vida.
5. No fim, guarda um TCX a partir do **resumo** (Strava, Intervals.icu e TrainingPeaks aceitam-no).

A descrição completa: **[Guia do utilizador](USER_GUIDE.md)**.

## O que faz

**Percursos**
- Ficheiro GPX (navegar, arrastar e largar, `Ctrl+O`), link do bikerouter/BRouter (o perfil, a alternativa e as opções de evitar
  também vêm com ele), percursos recentes, percurso de demonstração incluído.
- Leitor de GPX tolerante (BOM, espaços de nomes, vários segmentos, `<rte>`, uma mensagem compreensível para um ficheiro
  corrompido); a elevação em falta é preenchida pelo Open-Meteo. Deteção de subidas da 4.ª categoria a HC; os traçados de
  segmentos incluídos no ficheiro (por exemplo, um segmento do Strava) aparecem como bandas.

**Pedalar**
- Física realista (Martin et al. 1998: gravidade, resistência ao rolamento, atrito dos rolamentos, arrasto aerodinâmico das
  rodas com raios, inércia); o rolo recebe o declive do troço à tua frente (até ±20 %, com uma intensidade de 0–150 %).
- **Rolo FTMS** (subidas e ERG), **potenciómetro** (CPS), **velocidade/cadência** (CSC), **monitor de frequência cardíaca** (FC); um
  único dispositivo físico pode desempenhar *várias funções* numa só ligação; **Zwift Click/Play** como caixa de velocidades virtual; **ciclista simulado**.
- **Caixa de velocidades virtual**: transmissão 2 × 12, 23 velocidades, modifica a resistência do rolo.

**Treinos e análise**
- 93 treinos incluídos em 12 categorias, com etiquetas, um gráfico de blocos colorido por zona e 943 mensagens do treinador
  cronometradas; importação de treinos do MyWhoosh e do Zwift (`.zwo`, XML, JSON).
- Zonas de Coggan, NP, IF, TSS, kcal; zonas de frequência cardíaca; gravação segundo a segundo.
- **Fantasma** (a última volta, a melhor, ou as próprias marcas temporais do GPX) e **segmentos locais com PR**.
- **Revisão e comparação de voltas**: as voltas feitas no mesmo percurso (no máximo quatro) lado a lado por potência, frequência
  cardíaca, cadência, velocidade e diferença de tempo; podes ampliar um troço (arrastando ou a partir das subidas) e consultar a tabela do troço.
  **Importação de uma volta GPX ou TCX** de uma exportação do Strava ou do Garmin, com frequência cardíaca e cadência; se não tiver watts, a
  física da simulação estima-os ao contrário (com o teu peso corporal ou o indicado, bicicleta e resistência ao rolamento).
- **Página de potência**: potência de pico de 5 s a 60 minutos, uma sugestão de FTP a partir dos teus melhores 20 minutos (definível com um
  botão), carga semanal e mensal. Só contam os watts medidos.
- **Cópia de segurança**: os percursos, as voltas, os treinos importados e as definições num único ficheiro zip (sem a chave da Google),
  com uma reposição que funde os dados. Nada sai para a rede.
- **Exportação TCX** (esquema Garmin): Strava, Intervals.icu e TrainingPeaks aceitam-no quando é enviado à mão. Não há
  envio integrado: o início de sessão no Strava teria de ser repetido depois de cada volta, por isso removemo-lo (0.3.2).

**Interface**
- **Disposição em mosaico**: os painéis funcionam como num gestor de janelas em mosaico (sem sobreposição, sem espaços vazios); podem ser
  redimensionados, movidos e fechados arrastando; uma disposição separada com e sem treino.
- Tema escuro e claro (ambos completos), tamanho do texto 80–160 %, **a interface fala húngaro, inglês, espanhol e português**
  (*Definições → Aparência → Idioma*, ou seguindo automaticamente o idioma do sistema), tipos de letra incorporados.
- **Telemóvel e tablet**: uma vista com separadores; o separador “Pedalada” tem tudo o que precisas numa volta.
- **Street View** opcional (com a tua própria chave da Google, transferência offline) e um **painel Navegador** (Windows, incorpora o
  Firefox instalado).
- Mantém a máquina acordada enquanto pedalas (Windows, Linux, macOS).

## Capturas de ecrã

<table>
  <tr>
    <td width="50%"><img src="../../screenshots/ride-light.png" alt="Tema claro"><br><sub><b>Tema claro</b>: o tema branco-papel está em pé de igualdade com o escuro</sub></td>
    <td width="50%"><img src="../../screenshots/workout.png" alt="Treino em modo ERG"><br><sub><b>Treino em modo ERG</b>: cartão do treino, gráficos em direto, gráfico de blocos</sub></td>
  </tr>
  <tr>
    <td><img src="../../screenshots/workout-picker.png" alt="Seletor de treinos"><br><sub><b>93 treinos</b> com filtros e um gráfico de blocos colorido por zona</sub></td>
    <td><img src="../../screenshots/summary.png" alt="O resumo da volta"><br><sub><b>Resumo</b>: NP, IF, TSS, segmentos, zonas; exportação TCX</sub></td>
  </tr>
  <tr>
    <td><img src="../../screenshots/devices.png" alt="Janela de dispositivos"><br><sub><b>Dispositivos</b>: um dispositivo em várias funções</sub></td>
    <td align="center">
      <img src="../../screenshots/phone-ride.png" width="170" alt="Telemóvel, separador Pedalada">
      <img src="../../screenshots/phone-map.png" width="170" alt="Telemóvel, separador Mapa"><br>
      <sub><b>No telemóvel</b>: separadores Pedalada e Mapa</sub>
    </td>
  </tr>
  <tr>
    <td><img src="../../screenshots/review.png" alt="Revisão de treinos"><br><sub><b>Revisão</b>: duas voltas numa subida, a segunda com watts estimados</sub></td>
    <td align="center">
      <img src="../../screenshots/review-phone.png" width="170" alt="Telemóvel, revisão">
      <br><sub><b>No telemóvel</b>: separadores Voltas, Mapa, Gráfico, Troço</sub>
    </td>
  </tr>
</table>

<sub>As capturas de ecrã mostram um percurso real do BRouter (os últimos 12 km do percurso Hűvösvölgy–Dobogókő que está no repositório
como dados de teste), com o ciclista simulado. Percurso e mapa: © colaboradores do OpenStreetMap, ODbL. As imagens são produzidas por
`node scripts/readme_shots.mjs`.</sub>

## Dispositivos e estado

**Nem todos os pontos foram testados com hardware real ou com um serviço real.** Com honestidade:

| Item | Estado |
|---|---|
| Rolo inteligente FTMS, **ERG** (treino) | testado, **também com um rolo real** (um modelo: VE200) |
| **Simulação de subidas** FTMS | testada a fundo (com um transporte simulado, usando os vetores de bytes da especificação); **ainda não confirmada com um rolo real** |
| Monitor de frequência cardíaca, potenciómetro separado, sensor CSC | os descodificadores estão testados; os dados CPS/CSC do rolo são reais; ainda não foi testado com um sensor separado |
| Zwift Click / Play | implementado (não oficial), ainda não testado com um comando real |
| Ciclista simulado | funciona sem hardware, em todas as plataformas |
| Windows | em uso diário |
| Linux, macOS | a CI compila-os e (no Linux) corre um teste rápido de 20 segundos; ainda sem verificação numa máquina real (experiências são bem-vindas) |
| Android | a interface já correu (os erros da 0.3.0 foram encontrados a correr); o seletor de ficheiros do sistema está implementado; **o Bluetooth ainda não foi testado num dispositivo** |
| Street View | implementado e testado com um servidor de teste local; ainda não testado com o serviço real |

Detalhes: [`docs/TESTING.md`](../../TESTING.md) e [`docs/BLUETOOTH.md`](../../BLUETOOTH.md). Agradecemos relatórios de erros,
experiências e dispositivos compatíveis ou incompatíveis nas Issues (vale a pena anexar o `ble.log`).

## Documentação

| Para | O quê |
|---|---|
| **Utilizadores** | [Guia do utilizador](USER_GUIDE.md) · [Privacidade e tráfego de rede](PRIVACY.md) · [Registo de alterações](../../../CHANGELOG.md) · [Húngaro](../../../README.md) · [Espanhol](../es/README.md) · [Português](README.md) |
| **Programadores** | [Arquitetura](../../ARCHITECTURE.md) · [Guia de desenvolvimento](../../DEVELOPMENT.md) · [Testes](../../TESTING.md) · [Lançamento](../../RELEASING.md) · [Registo de decisões](../../DECISIONS.md) · [Traduções (i18n)](../../I18N.md) |
| **Temas** | [Bluetooth](../../BLUETOOTH.md) · [Simulação e física](../../SIMULATION.md) · [Percursos e formatos de dados](../../ROUTES_AND_DATA.md) · [Interface](../../UI.md) · [Sistema de design](../../DESIGN.md) · [Android](../../ANDROID.md) · [Street View](../../STREETVIEW.md) · [Antecedentes e código reutilizado](../../PRIOR_ART.md) |

O índice completo: [`docs/README.md`](../../README.md).

## Desenvolvimento

```powershell
cargo run -p ride-app                      # compilação de depuração e arranque (Windows, Linux, macOS)
cargo test --workspace                     # a bateria de testes completa (2026-11-06: 1076 testes)
node scripts/readme_shots.mjs              # regenerar as capturas de ecrã
```

Um espaço de trabalho Rust com dez crates de biblioteca e a aplicação (`ride-app`):

```
crates/
  ride-model         modelo de dados comum (Route, Settings, RideSnapshot, Workout, …) – o “contrato”
  ride-route         GPX/bikerouter → Route, subidas, persistência, registo de voltas
  ride-sim           física, RideEngine, executor de treinos, fantasma, segmentos, TCX (sem I/O e sem relógio)
  ride-ble           hub BLE (FTMS/CPS/CSC/HR/Zwift) + ciclista simulado
  ride-ui-kit        sistema de design: tokens, paleta, ícones, formatação
  ride-ui-map        mapa de tiles · ride-ui-profile  perfil e gráficos
  ride-ui-hud        instrumentos, cartões, controlos       · ride-ui-dialogs  diálogos
  ride-ui-streetview painel Street View
  ride-app           a estrutura: estado, controlador, disposição em mosaico, proxy de tiles, atualizador, Android
packaging/  scripts/  tests/fixtures/  docs/
```

O código assenta em Dioxus 0.7 (interface baseada em webview) e em `btleplug`. No Windows, o desenvolvimento decorre na cadeia de ferramentas GNU
(o exe de lançamento é compilado pela CI com MSVC). Pré-requisitos, parâmetros (`RIDE_*`), depuração, empacotamento:
[`docs/DEVELOPMENT.md`](../../DEVELOPMENT.md). A descrição do projeto para o Claude Code: [`CLAUDE.md`](../../../CLAUDE.md).

**Contribuir.** Agradecemos relatórios de erros, registos de medições e alterações de código. As regras em resumo: só pode ser portado código com uma licença
permissiva (ver abaixo), a fonte do texto visível para o utilizador é o húngaro (cada string da interface é uma chave em
`crates/ride-i18n/locales/hu.json`, com as traduções ao lado; ver [`docs/I18N.md`](../../I18N.md)), nunca deixamos que dados externos
provoquem um pânico, e uma funcionalidade nova vem com um teste e documentação. O código submetido entra sob a licença do
projeto (MIT ou Apache-2.0), a menos que indiques outra coisa.

## Licença

### O código do próprio projeto

O código do próprio `bike-osm-ride` tem **dupla licença: [MIT](../../../LICENSE-MIT) OU [Apache-2.0](../../../LICENSE-APACHE)**,
conforme preferires (SPDX: `MIT OR Apache-2.0`). Se contribuíres, o código submetido entra sob estas mesmas duas licenças,
a menos que indiques outra coisa.

### Código aberto reutilizado

Usamos deliberadamente apenas código com licenças permissivas. A origem e a licença são preservadas nos ficheiros de origem e em
[`NOTICE.md`](../../../NOTICE.md).

| Origem | Licença | Para que o usamos |
|---|---|---|
| crate [`ftms`](https://github.com/deancochran/ftms) | MIT | codificação e descodificação dos dados e dos comandos de controlo FTMS Bluetooth (rolo inteligente) |
| [`cycling-ble`](https://github.com/friedrichwilken/cycling-ble) | Apache-2.0 OR MIT | interpretação dos dados de potenciómetros, cadência e frequência cardíaca (integrado, marcado nos ficheiros de origem) |
| [OsmolControl](https://github.com/Parapeth962/OsmolControl) | MIT | ligação ao rolo e filtro de segurança dos comandos (partes portadas, marcadas) |
| [iron-heart](https://github.com/nullstalgia/iron-heart) | MIT | pesquisa e reconexão do monitor de frequência cardíaca (padrão tomado, sem código, marcado) |
| Tipos de letra [Barlow e Barlow Condensed](https://github.com/jpt/barlow) | SIL OFL 1.1 | o texto e os numerais da interface; **incorporados** no programa (WOFF2 em subconjunto, Latin + Latin Extended, cobrindo o húngaro), por isso funcionam sem rede; o texto da licença: [`crates/ride-ui-kit/assets/fonts/OFL.txt`](../../../crates/ride-ui-kit/assets/fonts/OFL.txt), detalhes em [`NOTICE.md`](../../../NOTICE.md) |
| [virtual-cog](https://github.com/pymmog/virtual-cog) | Unlicense (domínio público) | o protocolo BLE do Zwift Click/Play e do Zwift Hub (reescrito em Rust): o módulo `zwift` de `ride-ble`, detalhes em [`NOTICE.md`](../../../NOTICE.md) |
| [`p256`](https://github.com/RustCrypto/elliptic-curves), [`hkdf`](https://github.com/RustCrypto/KDFs), [`sha2`](https://github.com/RustCrypto/hashes), [`aes`](https://github.com/RustCrypto/block-ciphers), [`ccm`](https://github.com/RustCrypto/AEADs) (RustCrypto) | MIT / Apache-2.0 | a troca de chaves do Zwift Click (ECDH P-256, HKDF-SHA256) e a cifra AES-CCM |
| [Mozilla Firefox](https://www.mozilla.org/firefox/) | MPL-2.0 (o programa **não o inclui**, lança a cópia instalada na máquina) | o painel Navegador: a janela do Firefox instalado é incorporada, com um perfil separado |
| [Dioxus](https://dioxuslabs.com), [btleplug](https://github.com/deviceplug/btleplug), [tokio](https://tokio.rs), [quick-xml](https://github.com/tafia/quick-xml) e as restantes dependências Rust | maioritariamente MIT / Apache-2.0 | a lista completa, gerada automaticamente (638 pacotes): [`THIRD_PARTY_LICENSES.md`](../../../THIRD_PARTY_LICENSES.md) |

Serviu apenas como **referência** (sem retirar código): um valor numérico de um exemplo resolvido da implementação aberta
Martin chamada `rouleur` no teste de física, [`nrenner/brouter-web`](https://github.com/nrenner/brouter-web) (MIT)
para verificar o formato do link do bikerouter, e as notas de engenharia inversa do Makinolo sobre o protocolo do Zwift Click/Play como
documentação: <https://www.makinolo.com/blog/2024/10/20/zwift-trainer-protocol/>. Este é um trabalho de interoperabilidade não oficial:
não temos qualquer ligação à Zwift, o programa não é um produto Zwift e a Zwift não o suporta.

**Não** usámos código de projetos com licença GPL/AGPL (por exemplo, OpenCycling, suffice, GoldenCheetah, QDomyos-Zwift),
porque isso colocaria todo o programa sob copyleft; os detalhes da investigação:
[`docs/PRIOR_ART.md`](../../PRIOR_ART.md).

**Auditoria de licenças (2026-11-04, `cargo metadata --filter-platform`, dependências normais e de compilação).** O número de crates externos
(crates.io) por plataforma: Windows (GNU) 396, Linux 458, macOS (Apple Silicon) 400, Android (aarch64) 449. Entre eles
**não há GPL, AGPL nem LGPL, e nenhuma licença desconhecida**; as licenças permissivas são MIT, Apache-2.0, BSD, ISC, Zlib,
Unicode, CC0, Unlicense e semelhantes.

**O único tipo de copyleft é o MPL-2.0**, que é **ao nível do ficheiro**, e usamos os crates sem alterações do crates.io (sem
código vendorizado); o código-fonte está disponível nos repositórios indicados em [`THIRD_PARTY_LICENSES.md`](../../../THIRD_PARTY_LICENSES.md):

- no Linux e no macOS, `option-ext` (através da cadeia `dioxus-desktop → tray-icon → dirs`);
- no Android, `selectors`, `cssparser`, `cssparser-macros` e `dtoa-short` (através da cadeia `wry → kuchikiki`);
- no Windows, nenhum.

Isto não afeta a licença do próprio projeto: o nosso código continua a ser `MIT OR Apache-2.0`. O `Cargo.lock` contém as
dependências de todas as plataformas; para regenerar a lista: `.\scripts\third_party_licenses.ps1`, e os comandos de auditoria por plataforma estão
no ponto 9 de [`docs/DEVELOPMENT.md`](../../DEVELOPMENT.md).

### Mapas e fontes de dados (atribuição)

A aplicação chama serviços externos enquanto corre; os dados deles podem ser usados ao abrigo das suas próprias condições, e a aplicação
mostra as atribuições exigidas no mapa (a lista completa das chamadas: a [política de privacidade](PRIVACY.md)):

- **Dados do mapa:** © colaboradores do [OpenStreetMap](https://www.openstreetmap.org/copyright), [ODbL 1.0](https://opendatacommons.org/licenses/odbl/1-0/).
- **Tiles:** os tiles padrão do [OpenStreetMap](https://wiki.openstreetmap.org/wiki/Tiles) (`tile.openstreetmap.org`). Os estilos de mapa *Escuro* e
  *Claro* filtram os mesmos tiles localmente (filtragem CSS), por isso não é precisa nenhuma chave de API nem um servidor de estilos de terceiros.
  Os tiles transferidos ficam em cache local para poupar os servidores ([política de utilização dos tiles do OSM](https://operations.osmfoundation.org/policies/tiles/));
  não há pré-carregamento em massa.
- **Street View (opcional):** serviços da [Google Maps Platform](https://mapsplatform.google.com): o endpoint de *metadados* da
  Street View Static API (apenas JSON: estado, id do panorama, posição), o endpoint de imagens da Static API (só para a transferência
  offline que pedires) e a vista Street View da Maps Embed API. O programa usa **a tua própria** chave de API (a tua conta Google Cloud,
  a tua quota e faturação); a chave está apenas na tua máquina, em `settings.json`, nunca é registada e só é enviada para
  `maps.googleapis.com` / `www.google.com`. Aplicam-se à utilização os [termos da Google Maps Platform](https://cloud.google.com/maps-platform/terms) e as
  [políticas do Street View](https://developers.google.com/maps/documentation/streetview/policies). O programa
  **não transfere imagens por omissão**: só quando carregas no botão *Transferir* do painel é que guarda imagens do percurso
  na tua máquina, por um máximo de 30 dias; mostra-as apenas sem chave ou quando há um erro de rede e, caso contrário, é mostrada a vista
  ao vivo em resolução total. O armazenamento permanente de imagens do Street View é restringido pelos termos da Google, por isso a decisão
  e a responsabilidade são tuas; a transferência usa a quota da tua própria conta Google. As políticas permitem guardar *ids de panorama* (estes podem mudar
  com o tempo, por isso a coordenada e a marca temporal ficam guardadas ao lado deles, e a vista incorporada também recebe a coordenada ao lado
  do id). Este programa não é um produto Google e não tem qualquer ligação com a Google.
- **Descrições dos treinos:** a biblioteca de treinos incluída é texto nosso, editado a partir de protocolos de treino públicos (British Cycling,
  TrainingPeaks, CTS, FasCat, TrainerRoad e outros) e de estudos de ciência do treino (Rønnestad, Seiler, Helgerud, Billat, Gibala,
  Bossi, Karsten e outros); as fontes usadas aparecem por treino no campo `sources` de
  `crates/ride-model/assets/workouts.json`.
- **Planeamento de percursos:** [BRouter](https://brouter.de) / [bikerouter.de](https://bikerouter.de) (serviço público; só o chamamos se
  colares um link do bikerouter, e o percurso é calculado pelo servidor deles). Os ficheiros `tests/fixtures/brouter_*.gpx` são respostas do BRouter,
  a partir de dados do OSM (ODbL), e o mesmo percurso aparece nas capturas de ecrã do README.
- **Preenchimento de dados de elevação em falta (opcional):** [Open-Meteo](https://open-meteo.com) Elevation API,
  [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/), para uso não comercial.

A nossa própria licença não cobre esses dados nem esses serviços.

### Aviso legal

O software é disponibilizado “tal como está”, sem garantia; os detalhes estão nos ficheiros [LICENSE-MIT](../../../LICENSE-MIT) e
[LICENSE-APACHE](../../../LICENSE-APACHE). O controlo dos dispositivos (o ajuste automático da resistência do rolo) é por tua conta e
risco.
