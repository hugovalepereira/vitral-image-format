# 04 — Plano de ação

Princípio orientador: **ser útil já, sem esperar por ninguém.** Os browsers só
adotam formatos novos ao fim de muitos anos (o JPEG XL foi normalizado em 2021,
retirado do Chrome em 2022 e só voltou em 2026). Por isso o Vitral tem de ter
valor no primeiro dia através de três peças:

* o PNG como contentor;
* um polyfill para a web;
* plugins para as ferramentas de design.

A normalização vem depois, *se* houver tração.

```
Fase 0 ─ Fase 1 ─ Fase 2 ─┬─ Fase 3 ─ Fase 4 ─ Fase 5 ─ Fase 6
fundação  spec+core  web   │  Figma    outras   v2       normalização
                           └─ Promoção (contínua a partir da fase 2)
```

## Fase 0 — Fundação ✅ (esta sessão)

* [x] Estado da arte ([01](01-estado-da-arte.md))
* [x] Requisitos ([02](02-requisitos.md))
* [x] Decisões: nome, contentor, modelo, stack ([03](03-decisoes.md))
* [x] Plano (este documento)

## Fase 1 — Especificação v0.1 + implementação de referência (Rust) ✅

**Objetivo**: um formato definido ao bit e uma implementação que o prova.

* [x] `SPEC.md` v0.1: modelo K/W, chunk `viTr`, codificação residual, CRC,
      fallback, pilha de renderização, conformidade.
* [x] Crate `vitral`: codificar/descodificar, compositor de referência, planos de
      renderização, conversão de modos W3C → K/W, composição de camadas.
* [x] CLI `vitral`: `encode`, `from-image`, `compose`, `decode`, `render`,
      `inspect`, `diff`.
* [x] Testes: ida e volta exata, deteção de corrupção, exatidão dos modos afins,
      erro da pilha ≤ 2.
* [x] Exemplos gerados (`examples/`).
* [x] *Fuzzing* do descodificador (300 000 mutações sem *panics*) e variantes de
      PNG (paleta, cinzento, entrelaçado). Ver [05](05-verificacoes-v1.md).
* [x] Cor nativa sobre fundo transparente (D9), chunk *safe-to-copy* (D2),
      armazenamento direto de K/W (D10), regra sRGB (D11), Display P3 (D13).
* [x] Vetores de conformidade publicados em [`conformance/`](../../conformance/).

**Critério de saída**: qualquer pessoa consegue criar um `.vitral.png` a partir
de dois exports (preto/branco) e vê-lo sobre qualquer fundo com `vitral render`.

## Fase 2 — Web: WASM + polyfill + demo 🟡 (núcleo feito)

* [x] Crate `vitral-wasm`, com bindings para o browser.
* [x] Polyfill `vitral.js`: transforma `<img data-vitral>` numa pilha de 3
      canvases com `mix-blend-mode`. O HTML continua válido e o fallback aparece
      sem JS.
* [x] Página de demonstração: o mesmo objeto sobre vários fundos, com
      comparação PNG ↔ Vitral e objetos arrastáveis.
* [x] Teste automático em Chromium, Firefox e WebKit (Playwright, no CI): o
      browser bate com o compositor Rust (erro máximo de 2 níveis, médio < 0,05).
* [x] Codificador na demo com as três exportações (preto, branco, transparente).
* [x] Demo publicada no GitHub Pages.
* [x] Testar um Vitral próprio na demo: abrir um ficheiro (botão, ou arrastá-lo
      para a comparação) ou levar o resultado do codificador para os fundos.
* [ ] **Seguinte**: publicação no npm e no crates.io.

**Critério de saída**: um developer integra o Vitral num site com uma linha
(`<script type="module" src="vitral.js">`).

## Fase 2.5 — Fechar a v1 (antes de pensar na v2)

Ver também a lista de pontos pequenos em [06 — Todo](06-todo.md).

* [x] Exportações reais do Figma, com multiply, screen, normal a 50% e os três
      combinados: reproduzidas sem diferenças.
* [ ] Exportações reais de Photoshop, Illustrator e After Effects.
* [x] Vetores de conformidade publicados ([`conformance/`](../../conformance/)).
* [ ] Compressão melhor (deflate ao nível do oxipng) e rever a meta RNF1.
* [x] Display P3 no formato, no CLI, no polyfill e na demo; luz linear recusada
      (D13).
* [ ] Documentar o uso em produção: servir sem otimização automática (CDNs e
      Next.js removem o chunk); otimizadores que o mantêm (optipng, oxipng,
      pngcrush, `zopflipng --keepchunks=viTr`).
* [ ] Desempenho: 1 megapixel em menos de 50 ms no browser.

## Fase 3 — Plugin Figma

* **Exportar**:
  * Seleciona-se um frame.
  * O plugin exporta-o sempre três vezes, sem o designer ter de pensar nisso:
    com fundo preto, com fundo branco e como PNG normal, com fundo transparente
    (`exportAsync` com `colorProfile: 'DOCUMENT'`, para manter o P3 dos
    ficheiros P3; ver D13).
  * Codifica em WASM e descarrega o `nome.vitral.png`.
* **Avisar** quando um grupo dentro do frame está em Normal: os modos lá dentro
  não chegam ao fundo, e o Vitral vai guardar isso tal como está.
* **Importar**: cria um grupo *pass-through* com 3 camadas de imagem (Difference,
  Screen, Multiply). Só é exato sobre fundos preenchidos (D9); o plugin deve
  avisar quando o frame não tem fundo.
* **Publicar na Figma Community**: é o primeiro grande canal de divulgação.

## Fase 4 — Outras ferramentas e vídeo

* **Illustrator** (ExtendScript/UXP): importar como grupo de 3 rasters com modos.
  Exportar via *action* preto/branco.
* **After Effects**: importar como 3 camadas com modos.
* **Photoshop** (UXP): importar e exportar.
* **Inkscape, GIMP, Krita**: scripts.
* **Vídeo Vitral**: adiado (decisão do autor, out/2026). Ver a secção seguinte.

## Vídeo Vitral (adiado; notas para quando chegar a vez)

**Problema.** Os overlays de motion (light leaks, fumo, fogo, poeira, lens
flares, grão) vendem-se como vídeo sobre preto com a instrução "usar em Screen",
ou sobre branco com "usar em Multiply". Os vídeos com alfa só fazem o modo
normal, e um efeito que escurece *e* brilha (fumo) não cabe num só ficheiro.

**Estado da arte do vídeo transparente (pesquisa de out/2026):**

* **Não há um formato universal.**
  * O WebM/VP9 com alfa funciona no Chrome, Firefox e Edge, mas não no Safari.
  * O HEVC com alfa funciona no Safari, mas não no Chrome. Os sites servem os
    dois formatos ([Rotato][rotato], [MDN][mdn-codecs]).
  * Um guia de agosto de 2026 diz que o WebM com transparência está a chegar ao
    Safari, mas não confirmei numa fonte da Apple ou do WebKit.
* **O AV1 não tem alfa nativo.** O AVIF resolve isso com um segundo fluxo, só de
  luminância, para o alfa ([Jake Archibald][archibald]).
* **A técnica que está a resolver o problema** é o ***stacked alpha***: um MP4
  normal (H.264 ou AV1) com a cor na metade de cima e o alfa na metade de baixo,
  juntadas no leitor com um *shader* de GPU.
  * O [`<stacked-alpha-video>`][stacked] (Jake Archibald) é um componente de
    cerca de 2 KB e diz que o AV1 dá ficheiros com metade do tamanho do VP9.
  * O [Hypervideo][hypervideo] é um serviço recente com SDKs abertos para web,
    iOS e Android sobre a mesma ideia, em H.264.
  * Uma limitação: em Safari, o AV1 só tem descodificação por hardware em
    aparelhos recentes (iPhone 15 Pro, Macs M3 e posteriores).

**Como o Vitral vídeo encaixaria:**

* **É a mesma técnica, com mais faixas.**
  * Um MP4 normal empilha K, W e a cor nativa (3 faixas, ou 2 se a nativa for
    aproximada).
  * Qualquer leitor sem suporte mostra a imagem empilhada; um leitor com suporte
    separa as faixas.
* **Na web**, um *shader* WebGL calcula as 3 camadas (difference, screen,
  multiply) por frame e desenha-as em 3 canvases com `mix-blend-mode`. É o
  polyfill atual, frame a frame.
* **No After Effects e no Premiere**, um script importa o vídeo como 3 camadas
  com os modos certos, usando os modos nativos dos programas.

**Riscos:**

* A compressão com perdas e o 4:2:0 estragam a precisão de K e W, sobretudo nas
  zonas que invertem (W < K). É preciso medir com AV1 e H.264 a qualidade alta.
* Muitas apps de vídeo misturam em luz linear: é necessária a flag de espaço de
  cor (v2).
* A sincronização: uma só faixa empilhada evita os problemas de sincronizar
  vários `<video>`.

**Primeiro protótipo:** converter um light leak (vídeo sobre preto + "usar em
Screen") num MP4 empilhado e mostrá-lo sobre uma página com o polyfill.

[rotato]: https://rotato.app/blog/transparent-videos-for-the-web
[mdn-codecs]: https://developer.mozilla.org/en-US/docs/Web/Media/Guides/Formats/Video_codecs
[archibald]: https://jakearchibald.com/2024/video-with-transparency/
[stacked]: https://github.com/jakearchibald/stacked-alpha-video
[hypervideo]: https://www.hypervideo.dev/

## Fase 5 — Vitral v2

* **Nível 2**: camadas com modos não-lineares (overlay, soft light, difference,
  HSL), renderizadas como camadas DOM com o respetivo `mix-blend-mode`.
* **Nível 3**: efeitos de vizinhança.
  * **Blur de fundo** com máscara por pixel: na web via `backdrop-filter`, no
    Figma via *background blur*.
  * Refração e distorção.
* **Contentor**: decidir entre PNG com mais chunks e ZIP (ver D2).
* **16 bits, HDR, Display P3, flag de luz linear, animação (APNG).**

## Fase 6 — Normalização (só com tração)

* **Registar o chunk público** junto do PNG WG/W3C. Passaria de `viTr` (privado)
  para `viTr` (público).
* **Propor ao WICG/CSSWG** um mecanismo nativo, por exemplo
  `<img blending="auto">` ou um descodificador que exponha K/W ao compositor. Não
  há código com acesso ao fundo, por isso o risco de *timing* é igual ao do
  `mix-blend-mode`.
* **Mapear para JPEG XL**, que já tem camadas Mul/Add e tem suporte no Chrome 155.

---

## Promoção (contínua, a partir da fase 2)

**Mensagem central** (revista em out/2026): *"Blend modes that survive
export."* O Vitral apresenta-se como uma **proposta** para manter
os modos de mistura dentro da imagem: o que faz multiply no Figma, no
Illustrator ou no Photoshop continua a fazê-lo no site, sem ninguém o ter de
refazer. *"O PNG tem transparência. O Vitral tem luz e filtro."* passa a
subtítulo.

* **Porquê**: a frustração existe nos fóruns da Adobe e do Figma há anos ("o
  modo não pode viajar com a imagem"), sobretudo na passagem do designer para
  o programador ou o cliente.
* **Sem prometer de mais**: nomear os modos ("multiply, screen e companhia",
  nunca "todos os modos"), dizer onde funciona ("onde o Vitral é suportado",
  "nunca pior do que o PNG de hoje") e mostrar os limites antes de alguém os
  descobrir. Evitar "any blend mode", "works everywhere" e "no code".
* **Proposta, aberta a quem quiser ajudar**: o objetivo é juntar pessoas interessadas no tema
  que ajudem a melhorar o formato. Os limites atuais (overlay, soft light,
  blur de fundo) são a agenda da proposta, não um segredo. Usar "proposal", não
  "experiment" (preferência do autor).
* **Quando**: a mensagem para designers rende mais com o plugin do Figma. Até
  lá, a frente principal é a web.

1. **A demo é o pitch**:
   * Página com um objeto Vitral arrastável sobre fotos e gradientes, lado a lado
     com o mesmo objeto em PNG.
   * GIF ou vídeo de 10 segundos para redes sociais.
2. **Canais por fase**:
   * **Fase 2**:
     * Show HN (Hacker News), r/webdev, r/web_design, r/rust.
     * Newsletters: CSS Weekly, Frontend Focus, This Week in Rust.
     * Artigo técnico "Colored transparency for the web".
   * **Fase 3**:
     * Responder nos fóruns da Adobe e do Figma onde se perguntou como exportar
       com o modo (lista em [06](06-todo.md#promoção)).
     * Figma Community, Product Hunt.
     * r/FigmaDesign, r/graphic_design, Dribbble/Behance (shots feitos com
       Vitral).
   * **Fase 4**:
     * Criadores e marketplaces de assets: overlays, light leaks, texturas,
       sombras.
     * Hoje vendem ficheiros com a instrução "usar em Screen/Multiply"; o Vitral
       torna-os *drop-in*. É provavelmente o melhor canal de adoção.
3. **Comunidade**:
   * Spec aberta no GitHub, com *issues* para propostas.
   * Exemplos e licença permissiva.
   * Talks em meetups (Lisboa, Porto) e em conferências de CSS e design.
4. **Métricas**:
   * Instalações do plugin Figma.
   * Downloads no npm e no crates.io.
   * Sites que usam o polyfill.
   * Estrelas no GitHub.
   * Ficheiros partilhados.

## Riscos e mitigação

| Risco | Mitigação |
|---|---|
| Adoção lenta: "mais um formato" | É um PNG válido, por isso não há custo de experimentar. O polyfill e os plugins dão valor imediato. |
| CDNs e otimizadores retiram o chunk | Fallback razoável + CRC. Documentar as opções que preservam chunks (ex.: `oxipng --strip safe`). |
| Isolamento por contextos de empilhamento no CSS | Documentar. O polyfill avisa em modo debug quando um antepassado isola. |
| Cores diferentes entre ferramentas | Mistura nos valores codificados do espaço de cor do ficheiro (sRGB ou Display P3, D13), como o Figma. Testes de referência. |
| Expectativa de "funcionar em todo o lado" | Comunicar claramente: funciona onde há suporte, degrada noutros sítios. |
| Tamanho dos ficheiros | Residual nulo nas zonas normais. Medir e otimizar (fase 2). |
| Fundo transparente (design tools) | Definido (D9): mostra a cor nativa. A pilha de 3 camadas só é exata sobre fundos opacos; nos programas de design é preciso um fundo preenchido. |
| CDNs que recodificam (sharp, Cloudinary) | Medido (D2, [05](05-verificacoes-v1.md)): removem o chunk. Servir os ficheiros sem otimização automática. |
| Exportações em Display P3 | Guardadas em P3 (D13). Nos browsers que misturam em sRGB, exatas sobre preto e branco e aproximadas no resto. |
