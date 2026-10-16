# 07 — Autopiloto

Uma tarefa agendada do Claude que, de hora a hora, olha para o estado do
projeto, escolhe o passo seguinte com mais valor e implementa-o neste branch
(`claude/autopilot-routine-qp9fq2`). Nada chega ao `main` sem o autor fazer
merge do PR de rascunho.

## Como funciona

* **Quando corre:** de hora a hora. Se o autor esteve ativo na última hora
  (mensagens no projeto, commits ou merges no repositório), a execução é
  saltada, para não se cruzar com o trabalho dele.
* **Sobre o quê:** o branch acumula o trabalho. Cada execução continua a partir
  do que as anteriores fizeram (lê este diário) e não as repete.
* **Quando o `main` muda:** cada execução junta o `main` ao branch. Se entrou
  trabalho novo no `main`, primeiro adapta as alterações do autopiloto a esse
  trabalho e volta a testar, e só depois avança.
* **Depois de um merge do PR:** o trabalho já está no `main`, e o autopiloto
  continua a partir daí com um PR de rascunho novo.

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
* Pode mudar o aspeto visual e o site quando for justificado. Pode mexer em
  ficheiros que outro PR aberto também altera, se for claramente o melhor
  passo; nesse caso indica no diário qual é o PR.
* Não repete trabalho que outra conversa do projeto tem em curso.
* Não publica nada fora do projeto e do PR: nada de fóruns, redes sociais,
  pacotes ou releases. A promoção fica em rascunhos dentro de `docs/`.
* Testa antes de enviar. Mudanças visuais são verificadas no Chromium, em
  desktop e telemóvel.
* Decisões que só o autor pode tomar são perguntadas; enquanto espera, avança
  com outra coisa.

## Instruções do Hugo

As instruções mais recentes do autor sobrepõem-se às regras acima.

* **2026-10-16:** correr muito mais vezes do que uma por dia, mas só quando ele
  não está a trabalhar; iterar sobre o trabalho já feito em vez de recomeçar do
  `main`; adaptar-se ao que ele juntar ao `main`; mudanças visuais permitidas
  quando justificadas.

## Diário

Uma entrada por execução, a mais recente em cima: o que foi feito e porquê.

### 2026-10-16 12h50 — Ritmo de hora a hora

A pedido do autor: passa a correr de hora a hora, salta quando ele está ativo,
acumula o trabalho no branch e pode mudar o aspeto visual.

### 2026-10-16 — Arranque

Criado o branch, este documento e o PR de rascunho.
