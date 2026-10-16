# 06 — Todo

Pontos que ficaram para depois, a maioria identificados na revisão de código do
PR #1 (out/2026). Os itens maiores da fase 2.5 continuam no
[plano](04-plano.md).

## Formato

- [x] **CRC mais tolerante** (decisão D12). O CRC (a "impressão digital" dos
  pixels da imagem principal, usada para detetar alterações) incluía a cor dos
  pixels totalmente transparentes. Essa cor é invisível, e alguns otimizadores
  (`oxipng -a`, `pngcrush -blacken`) limpam-na; o ficheiro perdia então o
  Vitral sem necessidade.
  - Correção: o CRC trata os pixels com alfa 0 como (0, 0, 0, 0)
    (SPEC §4.4).

## Cor

- [x] **Verificar o espaço de cor em todos os comandos** (decisão D13). Todos
  os comandos aceitam sRGB e Display P3 e mantêm o espaço de cor no ficheiro.
- [x] **Detetar imagens em luz linear** (decisão D13). Os ficheiros marcados
  como lineares (chunk `gAMA`, perfil ICC com gama 1,0 ou `cICP`) são recusados
  com uma mensagem clara.
- [ ] **Polyfill em browsers que misturam em P3.** O polyfill monta as camadas
  para mistura em sRGB. Num browser que mistura em P3, um ficheiro P3 seria
  exato se as camadas fossem P3, mas o JavaScript não consegue saber em que
  espaço o browser mistura. Testar no Safari de um Mac com ecrã P3.

## Polyfill (rever depois de analisar a demo online)

- [ ] **Mudanças de `src` ou `srcset`** numa imagem já tratada não são detetadas;
  ficam as camadas da imagem antiga.
- [ ] **O elemento que envolve a imagem** (`<span class="vitral">`) muda a
  estrutura da página. Pode partir seletores CSS como `.galeria > img` e
  frameworks como o React, que esperam encontrar a imagem onde a deixaram.
  Alternativas a estudar: um *custom element* (`<vitral-img>`), ou camadas
  posicionadas sem mexer na imagem.
- [ ] **`object-fit` e `object-position`** não são respeitados: as camadas
  esticam para a caixa da imagem.
- [ ] **`loading="lazy"`**: o polyfill descarrega e descodifica todas as imagens
  logo no início, incluindo as que estão fora do ecrã.

Já corrigido na mesma revisão:

- se a criação dos canvases falhar, a imagem continua visível como PNG normal;
- a memória do WASM é sempre libertada;
- as camadas cobrem só a área da imagem, sem `padding` nem `border`, e
  acompanham-na quando a página muda de tamanho;
- imagens `inline-block` continuam lado a lado.

## Demo

- [ ] Se o visitante escolher um ficheiro antes de os exemplos acabarem de
  carregar, o ficheiro dele é substituído pelo exemplo.
- [ ] Duas codificações seguidas podem terminar fora de ordem e mostrar o
  resultado antigo.

## Site (GitHub Pages)

- [x] **Ativar o site** (passo manual do autor): Settings → Pages → Deploy from
  a branch → `main`, pasta `/ (root)`. Está em
  <https://hugovalepereira.github.io/vitral-image-format/>.
- [ ] **Analisar o polyfill na demo online** e só depois decidir os itens da
  secção Polyfill.
- [ ] **Publicar com GitHub Actions** em vez de servir o repositório inteiro a
  partir da raiz do `main`.
  - O workflow compila o WASM no CI, e assim `web/pkg` deixa de estar no
    repositório.
  - Publica só o que o site precisa (`web/` e os exemplos), sem `docs/pt`,
    `SPEC.md` nem `crates/`.
  - O `index.html` da raiz, que hoje redireciona para `web/`, deixa de ser
    necessário.
  - Fazer antes de promover o projeto.

## Biblioteca

- [ ] `with_native` só valida a largura e a altura da imagem nativa, não o
  tamanho real dos dados. Um buffer errado faz o programa crashar em vez de dar
  um erro.
- [ ] **Eficiência:** o codificador comprime a imagem principal duas vezes (uma
  por cada forma de guardar o residual). Basta comprimi-la uma vez.
