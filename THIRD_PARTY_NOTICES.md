# Componentes de terceiros

O código próprio deste projeto é licenciado sob [MIT](LICENSE). Dependências, fontes e ferramentas externas mantêm suas licenças e avisos. Esta relação não muda a licença de nenhum componente.

## Codex oficial

`preparar.sh` obtém o pacote oficial `@openai/codex` 0.171.0 e os componentes correspondentes em `runtime/`. Esse runtime não integra o repositório de código-fonte. O Codex é um projeto da OpenAI sob [Apache License 2.0](https://github.com/openai/codex/blob/rust-v0.171.0/LICENSE); suas dependências podem ter termos adicionais.

Uma exportação que inclua o runtime deve preservar os arquivos de licença, os avisos de autoria e os demais notices distribuídos com os pacotes. Modificações e redistribuições precisam observar os requisitos da licença aplicável. A licença do Codex não concede uma assinatura ou autorização para explorar o serviço da OpenAI.

## Dependências Rust diretas

Versões e expressões de licença abaixo foram conferidas nos metadados dos pacotes correspondentes ao `Cargo.lock`. `OR` indica uma alternativa de licenciamento, não uma mudança para MIT de todo o projeto externo.

| Componente | Versão registrada | Licença declarada | Origem |
|---|---|---|---|
| eframe | 0.37.0 | MIT OR Apache-2.0 | [egui/eframe](https://github.com/emilk/egui/tree/main/crates/eframe) |
| egui_commonmark | 0.26.0 | MIT OR Apache-2.0 | [egui_commonmark](https://github.com/lampsitter/egui_commonmark) |
| rfd | 0.18.0 | MIT | [rfd](https://github.com/PolyMeilex/rfd) |
| image | 0.25.10 | MIT OR Apache-2.0 | [image-rs/image](https://github.com/image-rs/image) |
| serde | 1.0.229 | MIT OR Apache-2.0 | [serde](https://github.com/serde-rs/serde) |
| serde_json | 1.0.154 | MIT OR Apache-2.0 | [serde_json](https://github.com/serde-rs/json) |
| base64 | 0.22.1 | MIT OR Apache-2.0 | [rust-base64](https://github.com/marshallpierce/rust-base64) |
| chrono | 0.4.45 | MIT OR Apache-2.0 | [chrono](https://github.com/chronotope/chrono) |

Há dependências transitivas além dessa tabela. Em particular, `epaint_default_fonts` 0.37.0 declara `(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0`; as fontes incorporadas não estão cobertas apenas pela MIT do cliente. Veja a [origem das fontes e suas licenças](https://github.com/emilk/egui/tree/main/crates/epaint_default_fonts).

Antes de redistribuir um executável, reúna os textos de licença e avisos efetivamente exigidos pelo grafo compilado, inclusive das fontes e do runtime. Esta tabela informativa e seus links não substituem esses textos nem constituem inventário completo das dependências transitivas. Uma mudança no `Cargo.lock` ou no runtime exige nova conferência.

## Identidade e ferramentas de desenvolvimento

A identidade visual própria usa um símbolo original com a letra R; os logotipos da OpenAI não são a identidade deste projeto. OpenAI, ChatGPT e Codex são marcas dos respectivos titulares, citadas para descrever a integração. A MIT não concede direitos sobre essas marcas; consulte as [diretrizes da OpenAI](https://openai.com/brand/).

Rust, Cargo, Node.js, npm, Git e bibliotecas do sistema são ferramentas ou dependências externas. As instalações locais conservam seus próprios avisos e licenças.
