# Termos explicativos do RUST GPeitinhos

Atualizado em 23 de novembro de 2026.

Este documento explica o funcionamento e a relação com serviços externos. Não acrescenta restrições à [licença MIT](LICENSE), não exige uma aceitação adicional para exercer os direitos dessa licença e não substitui os contratos de terceiros.

## Código e dependências

A MIT autoriza usar, copiar, modificar e redistribuir o código próprio, inclusive comercialmente, mantendo os avisos exigidos. As dependências continuam sob suas respectivas licenças; veja [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

O software é fornecido nas condições descritas na MIT, sem promessa de desempenho, disponibilidade, suporte ou ausência de falhas. As limitações de garantia e responsabilidade operam dentro da legislação aplicável e não afastam direitos obrigatórios de consumidores, quando houver relação de consumo. A análise preliminar está em [docs/LEGAL.md](docs/LEGAL.md).

## Conta e serviços da OpenAI

Cada usuário utiliza sua própria conta. O projeto não vende assinatura, créditos ou modelos e não controla sua disponibilidade ou cota. O cliente usa o login oficial ChatGPT do Codex e bloqueia a rota por chave de API. Isso identifica a autenticação; não é uma garantia financeira para cada operação ou crédito eventualmente utilizado.

O acesso aos serviços segue os [termos aplicáveis da OpenAI](https://openai.com/policies/terms-of-use/), o plano da conta e as regras da organização, quando houver. O cliente não autoriza compartilhamento de credenciais, contorno de limites nem acesso a conteúdo ou sistemas alheios.

Segundo a [documentação do App Server](https://learn.chatgpt.com/docs/app-server), aplicativos locais ou de código aberto podem continuar usando essa autenticação; serviços comerciais ou hospedados não podem usá-la. Essa condição é da integração com a OpenAI, não uma proibição de uso comercial do código MIT. Uma oferta comercial ou hospedada precisa de uma integração permitida para esse uso.

## Arquivos, comandos e integrações

O agente pode ler e alterar arquivos e executar comandos conforme as permissões selecionadas. O modo padrão permite autonomia na pasta escolhida; acesso completo é uma escolha explícita. Autorizações e pedidos suportados são apresentados na interface. O usuário deve conferir a pasta de trabalho, as permissões e as ações solicitadas.

Ferramentas, skills e servidores MCP podem executar código ou acessar serviços externos. Disponibilidade e comportamento dependem do Codex, da configuração local e do provedor conectado. A interface não reproduz todos os recursos do desktop oficial. Os fluxos de dados são descritos em [PRIVACY.md](PRIVACY.md).

## Identidade

RUST GPeitinhos é um projeto independente. As referências a OpenAI, ChatGPT e Codex descrevem compatibilidade; essas marcas pertencem aos respectivos titulares. A MIT do projeto não concede direitos de marca, nem representa aprovação da OpenAI. Consulte as [diretrizes de marca](https://openai.com/brand/).

Atualizações deste documento explicam mudanças do projeto e não retiram os direitos concedidos às cópias já recebidas sob a MIT.
