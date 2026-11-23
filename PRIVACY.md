# Privacidade e fluxo de dados

Atualizado em 23 de novembro de 2026. Este aviso descreve a distribuição de código aberto do RUST GPeitinhos. Uma versão modificada ou um serviço criado por terceiros pode tratar dados de outra forma.

## No computador

A interface se comunica com um processo local do Codex por stdin/stdout. Ela carrega informações de conta, catálogo, skills e histórico para exibi-las. O histórico e as credenciais de autenticação são administrados pelo Codex na configuração dele; o cliente não copia o histórico para `data/` nem grava tokens ou cookies em suas preferências.

`data/preferences.json` guarda a pasta selecionada, nomes e caminhos de projetos e preferências de organização. Esses caminhos podem revelar informações pessoais ou de trabalho. Arquivos criados pelo agente, saídas de comandos e o histórico do Codex podem permanecer no computador após fechar a janela.

O código da interface não configura um servidor próprio do mantenedor para receber conversas ou telemetria. Isso não significa que a sessão inteira seja offline, nem desativa os registros e fluxos do Codex, do sistema ou dos serviços utilizados.

## OpenAI e conteúdo enviado

Ao conversar, o Codex envia as mensagens e o contexto necessário ao serviço da OpenAI. Esse contexto pode incluir imagens anexadas, conteúdo de arquivos lidos pelo agente e resultados de ferramentas. Outros anexos são referenciados pelo caminho para leitura pelo agente: referenciar um arquivo não impede que seu conteúdo seja usado em uma solicitação posterior.

Login, renovação de credenciais, catálogo e consulta de uso também podem exigir comunicação com a OpenAI. Retenção, uso para melhoria de modelos, administração de workspace e transferências internacionais dependem do serviço, plano, configurações e contratos aplicáveis. O aplicativo não garante retenção zero, exclusão imediata ou que conteúdo jamais seja usado para treinamento. Consulte a [política de privacidade da OpenAI para o Brasil](https://openai.com/pt-BR/policies/br-privacy-policy/), os controles da sua conta e as regras da sua organização.

## Ferramentas e outros fornecedores

Skills são instruções que podem levar o agente a executar ferramentas. Servidores MCP, plugins e comandos podem transmitir consultas, argumentos, arquivos ou resultados aos provedores acionados, conforme sua configuração e autorização. As políticas desses fornecedores também se aplicam.

O painel de terminal executa o comando localmente por meio do Codex, sem transformá-lo em uma mensagem ao modelo. O próprio comando pode acessar a rede quando o modo de acesso permitir. A revisão Git usa somente leitura e rede desativada.

O botão ChatGPT web abre um navegador externo, que administra sua própria sessão, cookies e dados. A preparação e a compilação baixam pacotes de registros npm/Cargo; esses registros recebem as solicitações de download.

## Controle e compartilhamento

- Revise pastas, anexos, integrações e permissões antes de executar tarefas com dados sensíveis.
- Para apagar preferências da interface, feche o aplicativo e remova o arquivo `data/preferences.json`. Isso não apaga projetos, histórico do Codex ou dados mantidos por serviços externos.
- Para sair da conta, use o logout do CLI oficial. Sair não equivale a excluir os dados já processados.
- Para administrar histórico e dados da OpenAI, use os controles oficiais da conta. Pedidos relativos a outro provedor precisam ser dirigidos a esse provedor.
- Não publique preferências, credenciais, conversas ou saídas privadas em issues. Diagnósticos e capturas podem conter metadados ou caminhos; revise-os antes de compartilhar.

Relatos públicos podem ser feitos nas [issues do projeto](https://github.com/jose190901/rust-gpeitinhos/issues), sem dados sensíveis. O mantenedor não opera a conta da OpenAI nem promete excluir dados sob controle de terceiros.

A distribuição não acompanha credenciais nem dados pessoais de usuários. A aplicação da LGPD depende do tratamento realizado; uso local ou código aberto não produz uma isenção geral. Veja [docs/LEGAL.md](docs/LEGAL.md).
