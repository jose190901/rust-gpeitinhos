# Interface do RUST GPeitinhos

## Direção

Interface desktop Linux centrada na conversa e na pasta de trabalho. Visual escuro, discreto e compacto, com símbolo R original e acento verde.

## Estrutura implementada

- Janela inicial de 1280 × 820, com tamanho mínimo de 900 × 640.
- Lateral fixa de 240 px: identidade, abas **Conversas** e **Projetos**, busca, ações da aba, conta/conexão e bloco **Limite restante**.
- Cabeçalho: modelo, raciocínio, acesso, seleção da pasta e controle **Terminal · Alterações**.
- Área central: histórico em Markdown e ferramentas em blocos recolhíveis.
- Editor inferior: rascunho, anexos, skills e envio/interrupção. **Enter** envia, **Shift+Enter** pula linha e **Ctrl+Enter** também envia. Durante um trabalho, a ação é **Enviar agora**.
- Painel inferior redimensionável, aberto por **Ctrl+J**, com abas **Terminal** e **Alterações**. Reaproveita a área da conversa e preserva o editor.
- Diálogos de arquivos e pastas fora da thread gráfica; janelas de skills/ajuda e modais para pedidos de autorização e entrada.

## Projetos e cotas

**Conversas** apresenta uma lista por atividade recente, com busca e indicação de filtro por projeto. O botão **Todos** remove o filtro. **Projetos** lista nome e caminho, permite adicionar uma pasta existente e abre um formulário para criar uma nova pasta com nome e diretório pai. Resultados e erros de criação têm estado próprio e não bloqueiam o desenho da janela.

O canto inferior da lateral apresenta cotas de assinatura de forma compacta. Cada janela mostra duração e percentual restante, com detalhes do reinício informado. Cotas específicas, aliases e fallback geral têm rótulos próprios. Contagem de resets ausente aparece como **não informado**, sem converter ausência em zero. **Detalhes de uso** expande uma área com rolagem e altura limitada para horários, cotas e créditos informados; o único comando desse bloco é atualizar a consulta.

## Terminal e revisão

O terminal mostra a pasta e o acesso do comando em execução. Alterar os seletores prepara o próximo comando; o contexto da saída anterior permanece identificado. A saída usa fonte monoespaçada e rolagem; há controles para copiar, limpar, executar, enviar entrada, enviar EOF e parar. O código de saída aparece quando o processo termina. Cada comando começa em uma nova sessão Bash; não é um emulador de terminal com PTY.

A aba **Alterações** atualiza a revisão ao abrir ou mudar de pasta. Mostra lista de arquivos e status Git, seleção do arquivo e diff/prévia textual. Separa staged e unstaged; arquivos novos têm prévia limitada e arquivos binários recebem indicação própria. A interface só consulta o repositório, sem controles para commit, stage ou descarte. Carregamento e erros pertencem à revisão, sem ocupar o estado de envio da conversa.

## Identidade e tokens

`assets/logo.png` é a fonte da logo da lateral e do ícone da janela e do lançador.

| Uso | Valor |
|---|---|
| Fundo principal | `#1D1F20` |
| Lateral | `#17191A` |
| Superfícies | `#26292A` |
| Texto principal | `#EAEDEB` |
| Texto secundário | `#A6B0AB` |
| Acento | `#90CCAB` |
| Bordas | `#3E4341` |
| Erro | `#F5A096` |

Tipografia do tema: corpo 15 px, botões 14 px, texto pequeno 12 px, títulos 21 px e código 13 px. Marca com 19 px. São usadas as fontes do egui.

## Comportamento

O motor não bloqueia a abertura da janela. Conta, catálogo e histórico têm estados próprios. O editor aceita um novo rascunho enquanto o agente trabalha; confirmações atrasadas não apagam edições posteriores.

Enviar fica indisponível durante preparação, carregamento da conversa ou autorização pendente. Com um turno ativo, **Enviar agora** permite orientar o trabalho após validar a conta e confirmar seu identificador. Se o turno terminar antes da aceitação, o texto permanece no rascunho e a interface informa o ocorrido. Cancelar durante a preparação impede o envio; após a aceitação, a interrupção usa o identificador do turno. Solicitações desconhecidas não são aprovadas automaticamente. Os seletores de modelo, raciocínio e acesso ficam bloqueados durante o trabalho; fora dele, as escolhas valem para o próximo envio.

Resultados assíncronos de histórico, skills e revisão Git são associados ao contexto solicitado. Mudanças de pasta ou seleção invalidam resultados anteriores. Caminhos longos, saídas e erros usam corte com consulta do texto completo ou rolagem. O painel identifica quando a saída anterior pertence a outra pasta ou política de acesso.

## Verificação da interface

Ao verificar mudanças, conferir foco do editor, Enter/Shift+Enter, rascunhos editados durante envio, troca de conversa durante trabalho, criação de projetos e cotas incompletas. Terminal e revisão precisam manter visíveis a pasta, a política de acesso e o contexto da saída. A execução dos testes e a verificação visual são registradas separadamente em `docs/`, quando realizadas.

Não há auditoria formal de acessibilidade. Manter foco por teclado, texto legível, estados claros e a distinção entre capacidades do CLI e recursos do desktop ainda indisponíveis.
