# Cómo contribuir

Gracias por tu interés. El proyecto necesita dos perfiles por igual:

- **Programadores** (Rust, SQLite, Tauri, MCP).
- **Profesionales de presupuestación e ingeniería**: validan reglas de
  cálculo, preparan casos sintéticos, prueban la compatibilidad con Presto
  y revisan informes. No hace falta saber programar.

## Flujo de trabajo

1. **Todo empieza en una Issue.** Usa la plantilla adecuada: tarea, error,
   validación de ingeniería o compatibilidad BC3.
2. Rama desde `main` con nombre `tipo/NNN-descripcion` (p. ej.
   `feat/010-importador-bc3`, `fix/031-redondeo-porcentajes`).
3. Commits pequeños con **firma DCO**: `git commit -s` añade
   `Signed-off-by: Nombre <correo>` y certifica que tienes derecho a aportar
   ese código bajo la licencia del proyecto ([developercertificate.org](https://developercertificate.org/)).
4. **Pull Request** contra `main` enlazando la Issue (`Closes #NN`).
5. Revisión:
   - Código: al menos una aprobación de un mantenedor.
   - Si la PR toca mediciones, costes, márgenes, rendimientos o redondeos:
     etiqueta `needs-engineering-review` y aprobación de un **revisor técnico**
     (ver `CODEOWNERS`).
6. Fusión por *squash* cuando la CI esté en verde.

## Definición de «hecho»

Una funcionalidad **no está terminada** hasta cumplir todo lo siguiente:

- [ ] Criterio de aceptación de la Issue cumplido y demostrable.
- [ ] Pruebas automáticas que cubren el caso normal, los límites y los errores.
- [ ] Si es económica: valores esperados **calculados a mano** y documentados
      en `docs/casos-validacion/` o en el propio test, con la operación visible.
- [ ] **Revisión técnica de ingeniería y presupuestación** aprobada cuando aplique.
- [ ] Documentación actualizada (`docs/`, ADR si cambia una decisión).
- [ ] Entrada en `CHANGELOG.md` bajo «Sin publicar».
- [ ] CI en verde en Linux y Windows.

## Estados del tablero (GitHub Projects)

`Backlog → Preparado → En desarrollo → Revisión técnica → Pruebas → Hecho`

- **Preparado**: criterio de aceptación claro y sin bloqueos.
- **Revisión técnica**: PR abierta esperando revisión de código e ingeniería.
- **Pruebas**: fusionado; pendiente de validación con casos reales autorizados o Presto 8.8.
- **Hecho**: validado y documentado.

## Etiquetas

| Tipo | Etiquetas |
|---|---|
| Área | `arquitectura`, `costes`, `mediciones`, `bc3`, `versiones`, `interfaz`, `informes`, `subcontratas`, `mano-de-obra`, `compras`, `mcp`, `seguridad`, `expertis`, `documentacion` |
| Prioridad | `P0-critical` (bloqueante), `P1-high`, `P2-medium`, `P3-low` |
| Comunidad | `good-first-issue`, `needs-engineering-review` |

## Reglas del proyecto

1. Cada tarea tiene su Issue; todo cambio de código entra por Pull Request.
2. Las funciones económicas llevan pruebas automáticas.
3. Cambios en mediciones, costes, márgenes y rendimientos requieren revisión técnica.
4. **No se suben presupuestos reales de clientes.** Los BC3 de prueba son
   sintéticos o cuentan con autorización escrita del titular.
5. Las decisiones de arquitectura se documentan como ADR.
6. `main` siempre compila y pasa las pruebas.
7. Cada versión publicada tiene registro de cambios.
8. Se comprueba la licencia de toda dependencia y de todo código reutilizado
   (`cargo deny check licenses`). No se copia código de otros proyectos sin
   abrir antes una Issue con la licencia de origen.
9. Cada hito tiene criterios objetivos de aceptación.

## Estilo de código

- `cargo fmt` y `cargo clippy -- -D warnings` sin avisos.
- Identificadores y mensajes de error en castellano (el dominio es español:
  *medición*, *partida*, *rendimiento*); comentarios explicando el porqué.
- Nada de `f64` en cálculos económicos: usa `Decimal` y `ppto_core::redondear`.
- Cabecera `// SPDX-License-Identifier: GPL-3.0-or-later` en cada fichero.

## Para profesionales sin experiencia en Git

Puedes contribuir sin instalar nada: abre una Issue con la plantilla
«Validación de ingeniería», describe el caso con sus números y adjunta una
hoja de cálculo con datos ficticios. Un mantenedor lo convertirá en prueba
automática y te pedirá que la revises.
