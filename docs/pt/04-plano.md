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

## Fase 1 — Especificação v0.1 + implementação de referência (Rust)

**Objetivo**: um formato definido ao bit e uma implementação que o prova.

* [ ] `SPEC.md` v0.1: modelo K/W, chunk `vITR`, codificação residual, CRC,
      fallback, pilha de renderização, conformidade.
* [ ] Crate `vitral`: codificar/descodificar, compositor de referência, planos de
      renderização, conversão de modos W3C → K/W, composição de camadas.
* [ ] CLI `vitral`: `encode`, `from-image`, `decode`, `render`, `inspect`.
* [ ] Testes: ida e volta exata, deteção de corrupção, exatidão dos modos afins,
      erro da pilha ≤ 2.
* [ ] Exemplos gerados (`examples/`).
* [ ] **Seguinte**: vetores de conformidade publicados e *fuzzing* do parser
      (`cargo fuzz`).

**Critério de saída**: qualquer pessoa consegue criar um `.vitral.png` a partir
de dois exports (preto/branco) e vê-lo sobre qualquer fundo com `vitral render`.

## Fase 2 — Web: WASM + polyfill + demo

* [ ] Crate `vitral-wasm`, com bindings para o browser.
* [ ] Polyfill `vitral.js`: transforma `<img data-vitral>` numa pilha de 3
      canvases com `mix-blend-mode`. O HTML continua válido e o fallback aparece
      sem JS.
* [ ] Página de demonstração: o mesmo objeto sobre vários fundos, com
      comparação PNG ↔ Vitral e objetos arrastáveis.
* [ ] Teste automático em Chromium (Playwright): o browser bate com o compositor
      Rust.
* [ ] **Seguinte**: *playground* (arrastar os PNG preto/branco e descarregar o
      Vitral), publicação no npm e no crates.io, GitHub Pages.

**Critério de saída**: um developer integra o Vitral num site com uma linha
(`<script type="module" src="vitral.js">`).

## Fase 3 — Plugin Figma

* **Exportar**:
  * Seleciona-se um frame.
  * O plugin troca o fundo para preto e depois para branco e faz
    `exportAsync` de cada versão.
  * Codifica em WASM e descarrega o `nome.vitral.png`.
* **Importar**: cria um grupo *pass-through* com 3 camadas de imagem (Difference,
  Screen, Multiply).
* **Publicar na Figma Community**: é o primeiro grande canal de divulgação.

## Fase 4 — Outras ferramentas e vídeo

* **Illustrator** (ExtendScript/UXP): importar como grupo de 3 rasters com modos.
  Exportar via *action* preto/branco.
* **After Effects**: importar como 3 camadas com modos.
* **Photoshop** (UXP): importar e exportar.
* **Inkscape, GIMP, Krita**: scripts.
* **Vídeo Vitral (exploratório)**: dois fluxos K/W, ou um vídeo com o dobro da
  altura, para light leaks e overlays que hoje se vendem com a instrução "usar em
  modo Screen".

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

* **Registar o chunk público** junto do PNG WG/W3C. Passaria de `vITR` (privado)
  para, por exemplo, `viTR`.
* **Propor ao WICG/CSSWG** um mecanismo nativo, por exemplo
  `<img blending="auto">` ou um descodificador que exponha K/W ao compositor. Não
  há código com acesso ao fundo, por isso o risco de *timing* é igual ao do
  `mix-blend-mode`.
* **Mapear para JPEG XL**, que já tem camadas Mul/Add e tem suporte no Chrome 155.

---

## Promoção (contínua, a partir da fase 2)

**Mensagem central**: *"O PNG tem transparência. O Vitral tem luz e filtro."*
Uma imagem que se mistura sozinha com o que está por trás.

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
| Cores diferentes entre ferramentas | Mistura em sRGB codificado, igual ao CSS e ao Figma. Testes de referência. |
| Expectativa de "funcionar em todo o lado" | Comunicar claramente: funciona onde há suporte, degrada noutros sítios. |
| Tamanho dos ficheiros | Residual nulo nas zonas normais. Medir e otimizar (fase 2). |
| Fundo transparente (design tools) | Indefinido na v1. As camadas usam alfa 0 onde não fazem nada, o que minimiza artefactos. |
