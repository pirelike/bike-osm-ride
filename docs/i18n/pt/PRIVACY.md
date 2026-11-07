# Privacidade e tráfego de rede

> **Tradução.** A versão autoritativa deste documento é a [página de privacidade](../../PRIVACY.md) húngara;
> também está disponível em [inglês](../en/PRIVACY.md) e em [espanhol](../es/PRIVACY.md).

O bike-osm-ride **não tem conta, nem telemetria, nem analítica, nem publicidade**. Os teus dados (voltas, definições,
dados dos sensores) ficam na tua máquina; o programa só chama pela rede aquilo que tu pedes. Esta página lista exatamente
o que pode sair e o que está na tua máquina. As afirmações baseiam-se no código-fonte (2026-11-04, versão 0.3.0).

## O que sai da tua máquina, e quando

| Endereço | Quando | O que recebe | Nota |
|---|---|---|---|
| `tile.openstreetmap.org` | quando o mapa pede tiles | as coordenadas do tile (zoom, x, y) e o nome do programa no User-Agent (`bike-osm-ride/0.1 (desktop app)`) | em cache local durante 30 dias; sem pré-carregamento em massa (por causa da política de tiles da OSM) |
| `brouter.de` | se colares um link do bikerouter | os pontos lidos do link (longitude, latitude), o perfil e as evitações | o percurso é calculado pelo servidor deles |
| `api.open-meteo.com` | só se o GPX não tiver dados de elevação utilizáveis | as coordenadas dos pontos de amostragem do percurso (no máximo 100 por pedido) | CC BY 4.0, para uso não comercial |
| `api.github.com` | no arranque, cinco segundos depois | a consulta da última versão, `User-Agent: bike-osm-ride/<version>` | fica em silêncio se falhar; **não pode ser desativada nas definições** |
| `github.com` (transferência) | só se carregares no botão *Atualizar* | a transferência do novo pacote | |
| `maps.googleapis.com`, `www.google.com` | só se tiveres ligado o Street View e indicado uma chave de API | a posição do ciclista (coordenada) e a **tua própria** chave da Google | aplicam-se os termos da Google; vê [`STREETVIEW.md`](../../STREETVIEW.md) |
| as páginas abertas no Firefox | só se usares o painel Navegador (Windows) | o que vês nele | um perfil do Firefox separado, o teu navegador normal fica intocado |

Não há mais nada. Não há início de sessão no programa, nem envio de relatórios de erros, nem estatísticas de utilização.
O tráfego Bluetooth só passa entre o rolo, os sensores e a máquina.

## O que está na tua máquina

Está tudo numa única pasta (`%LOCALAPPDATA%\bike-osm-ride`, `~/Library/Application Support/bike-osm-ride`,
`$XDG_DATA_HOME/bike-osm-ride`, no Android a pasta privada da aplicação). A estrutura está descrita em
[`ROUTES_AND_DATA.md`](../../ROUTES_AND_DATA.md); em resumo:

- **definições** (`settings.json`): peso corporal, FTP, pulso máximo, aspeto, disposição, a tua chave da Google API;
- **percursos e registo de voltas**: a versão processada dos percursos abertos, o registo completo segundo a segundo de
  cada volta terminada (posição, watts, pulso, cadência);
- **tiles**, as **imagens do Street View** transferidas (no máximo 30 dias) e os ids de panorama;
- **registos**: `ble.log` (os pacotes de dados em bruto do rolo e dos sensores: watts, pulso, comandos de controlo; não
  contêm o nome nem o endereço do dispositivo) e `crash.log` (erros capturados); ambos rodam e ficam na máquina;
- o perfil do Firefox do painel Navegador;
- os restos do envio para o Strava da versão **0.3.1 e anteriores**: as pastas `strava` e `strava-webview` (esta última
  tem os cookies do teu início de sessão no Strava). O programa já não as usa e não as elimina: podes eliminá-las à
  vontade.

**Cópia de segurança.** *Definições → Cópia de segurança* cria um único ficheiro zip com os teus percursos, as tuas
voltas, os teus treinos importados e as tuas definições. O ficheiro vai para onde o guardares (no Android com o diálogo
“Guardar como” do sistema); o programa não envia nada para lado nenhum nem o carrega. A chave da Google API **não está
nele**, e ao restaurar também não aceita a chave que venha do ficheiro; os tiles, as imagens do Street View e os registos
também não estão nele. O ficheiro contém os percursos exatos das tuas voltas e os teus dados de saúde (pulso, watts), por
isso trata-o como os teus outros ficheiros pessoais.

**Segredos.** O único segredo é a chave da Google API. Está em `settings.json` *em texto simples* (não encriptada), por
isso protege a pasta como os teus outros ficheiros pessoais. O programa nunca a regista, não acaba numa mensagem de erro
nem aparece na saída de depuração, e só sai para a Google (ou para um servidor de teste local em `127.0.0.1` durante o
desenvolvimento).

**Eliminação.** Eliminar a pasta faz desaparecer tudo. As imagens e os ids do Street View também podem ser eliminados
separadamente em *Definições → Street View*; o registo de voltas de um percurso removido da lista de recentes mantém-se.

### Ficheiros de voltas importados

**Importar uma volta** lê um ficheiro GPX ou TCX que escolhas (ou arrastes). O programa processa o ficheiro na tua
máquina e só guarda o resultado (o registo segundo a segundo: posição no percurso, elevação, velocidade, pulso, cadência,
watts) na pasta `rides`, tal como as tuas próprias voltas. O conteúdo original do ficheiro (por exemplo o nome e as
datas/horas escritos pelo Strava) não vai para mais lado nenhum, e nada sai para a rede. O cálculo dos watts estimados
também acontece localmente.

## Coisas que vale a pena saber

- A OpenStreetMap, a BRouter, a Open-Meteo, o GitHub e a Google veem os teus próprios pedidos (endereço IP, hora); é
  assim com qualquer cliente de rede.
- Atualmente não há nenhuma definição para desativar a verificação de atualizações. Se não a quiseres, bloqueia
  `api.github.com` com uma firewall; o programa não deixa de funcionar por causa disso.
- O programa não é um produto da Google nem da Zwift, e não tem qualquer ligação com elas.
