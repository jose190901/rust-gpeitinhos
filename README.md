# RUST GPeitinhos

Cliente desktop Linux de código aberto, com interface nativa em Rust e o Codex oficial em segundo plano. Usa o login ChatGPT do Codex para acessar os modelos disponíveis na sua conta. É um projeto independente, sem afiliação ou endosso da OpenAI.

A janela abre enquanto o motor carrega conta, modelos, conversas e skills. O aplicativo não precisa iniciar o desktop oficial. Rust torna a interface local nativa; o tempo de resposta do modelo continua dependendo dos servidores e da conexão.

## O que oferece

- Conversas locais do Codex, busca e projetos em uma aba separada, com criação de pastas ou cadastro de uma pasta existente.
- Respostas em streaming, anexos, skills e apresentação dos pedidos de autorização e formulários suportados.
- Envio de orientações ao mesmo turno enquanto o agente trabalha.
- Seletores de modelo, raciocínio e acesso; consulta dos limites, horários de renovação e resets informados pela conta.
- Terminal integrado por comandos e revisão Git somente leitura.
- Abertura do ChatGPT web no navegador padrão.

O Browser embutido, voz, prévias interativas e interfaces próprias dos plugins do desktop não estão implementados. Ferramentas e integrações dependem da configuração do Codex e dos serviços conectados. As conversas do ChatGPT web não são sincronizadas.

## Requisitos

- Linux com sessão gráfica X11 ou Wayland.
- Rust **1.100 ou superior**, Cargo e um compilador/linker de sistema.
- Node.js **16 ou superior** e npm.
- Git para clonar o repositório e usar a revisão de alterações.
- Bibliotecas e ferramentas de desenvolvimento exigidas pelo egui/eframe, rfd e pelo backend gráfico da sua distribuição.
- Bash para o terminal, `xdg-open` para abrir o navegador e Python 3 para exportar pacotes.
- Conta ChatGPT com acesso ao Codex e aos modelos desejados, além de conexão com a internet para o serviço.

O repositório contém código-fonte. Runtime, executáveis compilados, credenciais e dados de usuários não são distribuídos aqui.

## Instalação

### 1. Preparar as dependências do Linux

Em Ubuntu/Debian, este é um ponto de partida para compilação e execução:

```sh
sudo apt update
sudo apt install build-essential pkg-config git curl ca-certificates python3 \
  libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev \
  libssl-dev libwayland-dev libdbus-1-3 xdg-utils xdg-desktop-portal zenity
```

As bibliotecas XCB, teclado e SSL seguem as [instruções do eframe 0.37](https://github.com/emilk/egui/blob/0.37.0/crates/eframe/README.md). Os seletores de arquivos usam o backend padrão XDG Portal do [rfd 0.18](https://docs.rs/rfd/0.18.0/rfd/#xdg-desktop-portal-backend), que também precisa de um backend de portal compatível com seu desktop. Em um desktop GTK/Xfce sem esse backend instalado, por exemplo:

```sh
sudo apt install xdg-desktop-portal-gtk
```

GNOME e KDE podem já fornecer seus próprios backends. Em outras distribuições, instale os pacotes equivalentes pelo gerenciador local. O exemplo acima não é um instalador universal nem cobre diferenças de todas as versões do Linux.

### 2. Instalar Rust e Cargo

Use o instalador `rustup` conforme a [página oficial do Rust](https://rust-lang.org/tools/install/):

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
. "$HOME/.cargo/env"
rustc --version
cargo --version
```

O primeiro comando baixa e executa o instalador oficial; siga as instruções exibidas. A linha com `. "$HOME/.cargo/env"` carrega o ambiente no Bash/sh atual. Também é possível abrir um novo terminal após instalar. Confira se `rustc` informa **1.100 ou superior**. Se você já usa rustup e precisa atualizar:

```sh
rustup update stable
```

### 3. Instalar Node.js e npm

Instale uma versão **LTS mantida** pelo [site oficial do Node.js](https://nodejs.org/en/download), com npm. Node 16 é apenas o mínimo do lançador, não a versão recomendada para uma instalação nova. Depois confira:

```sh
node --version
npm --version
```

### 4. Baixar e compilar o aplicativo

```sh
git clone https://github.com/jose190901/rust-gpeitinhos.git
cd rust-gpeitinhos
./preparar.sh
./compilar.sh
```

`preparar.sh` instala o Codex CLI **0.171.0** na pasta `runtime/`, via npm, sem instalação global. `compilar.sh` compila a interface e gera `RustGPeitinhos`. As ferramentas de instalação baixam dependências dos respectivos registros.

## Configuração inicial

1. Na pasta clonada, execute o login oficial:

   ```sh
   ./runtime/node_modules/.bin/codex login
   ```

   Escolha **ChatGPT** e conclua a autenticação no navegador. Se esse Codex já estiver autenticado com sua conta, ele pode reutilizar a sessão local. O cliente exige essa rota e bloqueia chave de API; não inclui assinatura nem libera modelos ausentes na conta. Veja a [documentação oficial de autenticação](https://learn.chatgpt.com/docs/auth?surface=cli).

2. Abra a interface:

   ```sh
   ./iniciar.sh
   ```

3. Aguarde a conexão e escolha a pasta de trabalho no cabeçalho. Na aba **Projetos**, você também pode cadastrar uma pasta existente ou criar uma nova.
4. Confira **modelo**, **raciocínio** e **acesso** antes de enviar. **Somente leitura** restringe alterações; **Autonomia na pasta** é o padrão; **Acesso completo** amplia as permissões e precisa ser escolhido explicitamente. Durante um turno, esses seletores ficam bloqueados.
5. Para usar skills, instale ou configure as skills no próprio Codex conforme a [documentação oficial](https://learn.chatgpt.com/docs/build-skills). Você pode abrir o CLI local com `./runtime/node_modules/.bin/codex` e usar `$skill-installer` para instalar uma skill. No aplicativo, selecione a pasta do projeto, abra **Skills** e escolha as instruções para o envio. Se uma skill recém-instalada não aparecer, use **Recarregar conexão** ou reinicie o aplicativo. O botão seleciona skills disponíveis; ele não importa automaticamente todas as integrações do desktop.

O padrão é **GPT-6.2 Sol**, raciocínio **Alto**, com autonomia na pasta escolhida. Se o catálogo da conta não oferecer esse modelo ou esforço, o aplicativo pede uma escolha disponível. Astra pode ser selecionado manualmente. Não há troca silenciosa de modelo após erro.

## Atualizar e recompilar

Feche o aplicativo depois de terminar seus trabalhos. Na pasta clonada:

```sh
git pull --ff-only
./preparar.sh
./compilar.sh
./iniciar.sh
```

Se você modificou fontes locais, resolva suas alterações antes de atualizar; o comando acima não as descarta. O runtime fica fixado no Codex CLI **0.171.0** porque o protocolo é vinculado à versão. Atualizar um Codex global não atualiza esse runtime. Evite instalar `@latest` manualmente nessa pasta: uma troca de versão exige conferir compatibilidade e testes do cliente.

## Solução de problemas

| Sintoma | O que conferir |
|---|---|
| `cargo: command not found` ou `rustc` ausente | Instale Rust pelo rustup; abra um novo terminal ou carregue `. "$HOME/.cargo/env"` no Bash/sh. Confira a versão mínima. |
| `linker cc not found`, erro de `pkg-config` ou biblioteca ausente | Instale as ferramentas de compilação e os pacotes de desenvolvimento indicados acima. A mensagem identifica a biblioteca ausente; use o equivalente da sua distribuição. |
| `npm` ou `node` não encontrado | Instale Node LTS com npm e confirme que ambos estão no `PATH` do terminal. Depois rode `./preparar.sh` novamente. |
| Login obrigatório ou autenticação por API recusada | Execute o login do CLI local, escolha ChatGPT e depois **Recarregar conexão**. A interface não oferece fallback por chave de API. |
| Modelo indisponível ou limite atingido | Escolha um modelo retornado pelo catálogo da sua conta ou aguarde a renovação informada pelo serviço. O cliente não desbloqueia modelos nem amplia cotas. |
| Janela não abre | Inicie em uma sessão gráfica X11/Wayland e confira drivers/bibliotecas OpenGL. Rode `./iniciar.sh` no terminal para ver a falha. |
| Seleção de arquivos não aparece | Confira D-Bus, XDG Desktop Portal, backend GTK/GNOME/KDE e Zenity conforme a documentação do rfd. |

Ao pedir ajuda, informe versões e o erro após remover dados privados. Não publique credenciais, histórico ou o conteúdo de `data/`.

## Usar

Escolha uma pasta ou um projeto e inicie uma conversa. Selecionar uma conversa carrega seu histórico; enviar uma mensagem retoma o trabalho no Codex.

- **Enter** envia; **Shift+Enter** pula linha; **Ctrl+Enter** também envia.
- Durante um turno, **Enviar agora** acrescenta uma orientação. Se ele terminar antes de aceitar o envio, o texto permanece no rascunho para você enviar novamente.
- **Interromper** cancela a preparação ou solicita a interrupção do turno. Modelo, esforço e acesso ficam bloqueados durante o trabalho.
- **Projetos** permite criar uma pasta ou registrar uma existente. Selecionar o projeto também filtra as conversas pela pasta; **Todos** remove o filtro.
- **Limite restante**, no canto inferior da lateral, mostra dados da conta. **Detalhes de uso** expande as janelas e os resets. Atualizar apenas consulta: não resgata resets nem presume renovação pelo relógio.
- **Ctrl+J** abre **Terminal · Alterações**.

O terminal inicia um novo Bash por comando: `cd` e variáveis não persistem entre execuções. Há saída em streaming, entrada, EOF e interrupção, mas não há PTY nem suporte a programas de tela cheia. Seus comandos não são enviados a um modelo pelo painel de terminal.

A revisão Git consulta status e diferenças staged/unstaged, renomes e prévias limitadas de arquivos novos. Usa sandbox somente leitura, sem rede, e não oferece commit, stage ou descarte. O terminal usa o acesso selecionado; os modos restritos mantêm a rede desativada. **Acesso completo** depende de escolha explícita.

Fechar a janela encerra o motor iniciado por este cliente e suas ferramentas, interrompendo trabalhos em andamento.

## Dados e diagnóstico

As preferências ficam em `data/preferences.json`, incluindo pasta selecionada e projetos. Histórico e credenciais são administrados pelo Codex em sua própria configuração. Mensagens e conteúdos usados pelo agente podem ser enviados à OpenAI e às integrações acionadas; veja [PRIVACY.md](PRIVACY.md).

```sh
./iniciar.sh --diagnostico
./iniciar.sh --diagnostico-uso
```

Esses diagnósticos consultam conexão, catálogo ou cotas sem enviar uma mensagem ao modelo. Podem exibir metadados da conta; revise a saída antes de compartilhá-la. Para renovar o login, use o CLI oficial e depois **Recarregar conexão** na interface.

## Desenvolvimento e distribuição

```sh
cargo test --locked
./compilar.sh
./empacotar.sh ../RUST-GPeitinhos-pacote
```

A exportação exige runtime preparado e executável compilado. Cria uma pasta nova, recusando destinos existentes, e preserva os avisos e licenças do runtime oficial. Antes de redistribuir um binário, confira e inclua também os textos e avisos exigidos por todas as dependências compiladas, inclusive fontes; os notices npm não bastam para essa conferência. Veja [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

Preferências, histórico, credenciais e caches de compilação não entram na exportação. O pacote pode ser movido inteiro; após mover, abra `iniciar.sh`, pois o `.desktop` mantém o caminho original. O repositório público distribui apenas fontes e documentação, sem runtime ou binários.

Para relatar falhas, abra uma [issue](https://github.com/jose190901/rust-gpeitinhos/issues) com sua distribuição, versões e passos para reproduzir. Remova dados pessoais, tokens e conteúdo privado dos registros. Não há garantia de prazo de suporte ou de paridade com o aplicativo oficial.

## Licença e serviços externos

O código próprio está sob [MIT](LICENSE), permitindo uso, modificação e redistribuição, inclusive comercial, preservados seus avisos. Dependências têm [licenças próprias](THIRD_PARTY_NOTICES.md). A licença do código não concede assinatura, acesso a modelos ou direitos sobre marcas.

A autenticação App Server usada aqui é voltada a aplicativos locais ou de código aberto; a OpenAI declara que essa autenticação não é permitida para serviços comerciais ou hospedados. A licença MIT não altera essa condição do serviço. Consulte a [documentação oficial do App Server](https://learn.chatgpt.com/docs/app-server), os [termos explicativos](TERMS.md) e a [pesquisa jurídica preliminar](docs/LEGAL.md).
