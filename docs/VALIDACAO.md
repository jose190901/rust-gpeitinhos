# Validação da versão inicial

Verificação em 23 de novembro de 2026:

- 76 testes automatizados aprovados, sem falhas.
- Compilação otimizada concluída.
- Janela nativa verificada visualmente; carregamento de metadados e consulta de uso aprovados, sem enviar mensagem a um modelo nessa verificação.

A suíte cobre autenticação ChatGPT e bloqueio de API key, Enter/Shift+Enter, orientação do turno esperado, cancelamentos e rascunhos, troca de conversa durante streaming, respostas de histórico atrasadas, criação de projetos e tratamento de cotas incompletas. Também cobre terminal e revisão Git com arquivos temporários e transporte simulado.

Essas verificações não certificam todos os plugins, todas as distribuições Linux, acessibilidade ou equivalência ao desktop oficial. Browser embutido, voz e prévias interativas não estão implementados. Dados de contas, conversas, capturas e registros privados usados na validação não fazem parte deste repositório.
