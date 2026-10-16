# 07 — Autopiloto

Uma tarefa agendada do Claude que, todos os dias, olha para o estado do projeto,
escolhe o passo seguinte com mais valor e implementa-o neste branch
(`claude/autopilot-routine-qp9fq2`). Nada chega ao `main` sem o autor fazer
merge do PR de rascunho.

## Objetivo final

Num programa de design (Figma, Illustrator, Photoshop…), um objeto tem blend
modes aplicados em várias partes. Carrega-se em exportar e sai um Vitral. Esse
Vitral, colocado em qualquer sítio (de novo no Figma, na web, em qualquer
outro), mistura-se com o que está por trás exatamente como no programa de
origem.

Não é possível chegar lá de uma vez. Cada passo tem de ser realista e útil já,
sem nunca perder de vista este objetivo.

## Como escolhe o passo seguinte

Lê o plano ([04](04-plano.md)), o todo ([06](06-todo.md)), este diário, os PRs
abertos e as decisões recentes do autor. Pensa em todos os níveis:

* **formato e especificação** (modos que ainda não funcionam, cor, vetores de
  conformidade);
* **leitura e escrita** (biblioteca Rust, polyfill, outros ambientes);
* **ferramentas** (plugins para Figma e outros programas, CLI);
* **site e documentação**;
* **promoção**, como faria alguém com experiência em lançar este tipo de
  projeto: a quem mostrar, quando, com que exemplos.

Prefere passos pequenos e completos a passos grandes a meio.

## Regras

* Trabalha só neste branch. Não faz merge, não mexe no `main`.
* Não altera ficheiros que outro PR aberto está a alterar, nem repete trabalho
  que outra conversa do projeto tem em curso.
* Não publica nada fora do projeto e do PR: nada de fóruns, redes sociais,
  pacotes ou releases. A promoção fica em rascunhos dentro de `docs/`.
* Testa antes de enviar. Mudanças visuais são verificadas no Chromium, em
  desktop e telemóvel.
* Decisões que só o autor pode tomar são perguntadas; enquanto espera, avança
  com outra coisa.

## Diário

Uma entrada por execução, a mais recente em cima: o que foi feito e porquê.

### 2026-10-16 — Arranque

Criado o branch, este documento e o PR de rascunho. A primeira execução diária
escolhe o primeiro passo.
