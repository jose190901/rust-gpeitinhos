# RUST GPeitinhos

<!-- impeccable:product-schema 1 -->

## Plataforma e stack

Cliente desktop Linux nativo. Rust, egui/eframe 0.37 e renderização Glow; Markdown com egui_commonmark e diálogos de arquivos com rfd. O motor é uma instalação local do Codex CLI 0.171.0, pelo protocolo App Server em stdin/stdout.

## Propósito

Oferecer uma interface local simples para conversas, projetos, skills e ferramentas do Codex. A janela deve abrir sem aguardar o motor e continuar responsiva durante os trabalhos. Terminal, revisão de arquivos, consulta de limites e envio de orientações durante um turno fazem parte do fluxo principal.

## Decisões confirmadas

- Nome público **RUST GPeitinhos**, símbolo R original e interface escura com acento verde.
- Novos envios: GPT-6.2 Sol, raciocínio Alto e autonomia dentro da pasta escolhida.
- Seleção de modelo, esforço e acesso pelo catálogo real da conta. Astra pode ser escolhido manualmente; pedidos ao GPT-6.2 Astra não devem ser substituídos por outra versão silenciosamente.
- Autenticação da assinatura ChatGPT, sem fallback por chave de API.
- ChatGPT web no navegador padrão para configurar conta e integrações.

## Funcionamento e capacidades

A interface desenha a janela sem esperar a conexão do motor. Depois carrega conta, modelos, conversas e skills do contexto. Selecionar uma conversa consulta seu histórico; enviar uma mensagem retoma ou cria uma conversa oficial. Há streaming, anexos, atividade de ferramentas, autorizações e formulários suportados pelo protocolo.

**Conversas** e **Projetos** são visualizações separadas. A primeira oferece uma lista por atividade, busca e filtro por projeto. A segunda permite criar uma pasta nova ou registrar uma existente, guardando nome e caminho nas preferências locais. A migração dessas preferências preserva o histórico do Codex; projetos não são cópias das conversas.

**Enter** envia a mensagem, **Shift+Enter** insere uma quebra de linha e **Ctrl+Enter** continua aceito. **Enviar agora** usa `turn/steer` para orientar o mesmo turno em andamento, com identificador esperado e validação prévia da conta ChatGPT. Se ele terminar antes de aceitar o texto, o aplicativo preserva o rascunho e não reenvia automaticamente. Os seletores de modelo, raciocínio e acesso ficam bloqueados durante o trabalho; fora dele, as escolhas valem para o próximo envio.

O bloco **Limite restante** consulta dados reais de cota por janela e modelo, com fallback geral do Codex explicitamente identificado. Mostra horários informados de renovação e a contagem de resets disponível; dados ausentes aparecem como não informados. Atualizações esparsas preservam metadados conhecidos, e leituras completas substituem o retrato anterior. Não há resgate de resets nem recuperação de cota presumida pela passagem do tempo.

O painel **Terminal · Alterações** reúne duas funções locais:

- Terminal pelo `command/exec` oficial, com streaming UTF-8, stdin, EOF, interrupção e código de saída. Não cria turnos nem solicita inferência. Cada execução inicia um novo Bash, na pasta e com o acesso selecionados; não há sessão shell persistente, PTY ou programas de tela cheia. Rede desativada nos modos restritos; acesso completo depende de seleção explícita.
- Revisão Git de status, staged, unstaged, untracked e renomes; diff textual e prévia limitada dos arquivos novos. Comandos passam pelo mesmo motor, sempre em sandbox somente leitura e sem rede, inclusive diante de filtros configurados pelo repositório. Não há botões de commit, stage ou descarte.

## Distribuição e estado

O repositório público distribui fontes, documentação e identidade própria; não inclui runtime, binários ou dados de usuários. A preparação instala o CLI localmente e a compilação gera o executável.

O pacote exportado inclui executável, código-fonte, logo e runtime npm oficial. Descobre seus arquivos pela pasta do executável, podendo ser movido inteiro. O script de exportação cria destino novo e recusa sobrescrever distribuições existentes. O `.desktop` conserva o caminho da exportação; após mover, o ponto de entrada é `iniciar.sh`.

Não são exportados toolchain, caches, preferências, histórico, tokens, cookies ou credenciais. Node.js e as bibliotecas gráficas do sistema continuam sendo dependências. O login, as skills e os plugins são os disponíveis na instalação local de destino.

O histórico permanece sob administração do Codex. Preferências novas guardam a pasta escolhida, os projetos e sua organização em `data/`. Se a gravação falhar após criar um projeto, o aplicativo avisa e preserva a pasta criada. A conta é validada antes de enviar; mudança de autenticação invalida a preparação pendente. Pedidos cuja aceitação ficou incerta não são reenviados automaticamente. Fechar a janela encerra o motor e seus subprocessos.

O cliente é independente da OpenAI. Código, dependências, serviço e marcas têm condições distintas, descritas em [TERMS.md](TERMS.md), [PRIVACY.md](PRIVACY.md) e [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

## Limites

A paridade com todo o desktop não foi comprovada. Browser embutido, voz, prévias interativas e interfaces próprias de plugins não estão implementados. A disponibilidade de ferramentas depende do runtime e de suas conexões. Conversas do ChatGPT web não são sincronizadas.

A interface local não acelera a inferência do modelo nos servidores. Tempos observados de abertura ou resposta não são garantias de latência.

## Princípios

1. A janela permanece utilizável enquanto o motor carrega ou trabalha.
2. Mostrar capacidades reais, carregamentos e erros de forma clara.
3. Preservar rascunhos e histórico; evitar envios duplicados e mudanças silenciosas de rota.
4. Aplicar a política de acesso também aos comandos locais e à revisão Git.
5. Manter explícita a ausência dos recursos do desktop ainda não integrados.
