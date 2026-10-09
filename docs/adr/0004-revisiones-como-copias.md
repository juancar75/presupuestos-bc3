# ADR-0004: Revisiones como copias completas

- Estado: Propuesto
- Fecha: 2026-10-09
- Issue: P-009

## Contexto

Se necesita conservar el presupuesto original, variantes y revisiones, con
la garantía de que modificar R2 no altera R1, y saber quién cambió qué.

## Decisión

- Cada revisión es una copia completa de conceptos, descomposición y
  mediciones, enlazada con su revisión de origen.
- Las revisiones pueden **bloquearse** (p. ej. la ofertada); una revisión
  bloqueada rechaza escrituras.
- Toda escritura se ejecuta en una transacción y deja registro en
  `auditoria` (usuario, operación, valor anterior y nuevo).

## Alternativas consideradas

- *Diferencias (deltas) entre revisiones*: menos espacio, pero comparar y
  cargar exige reconstruir cadenas; un error en un delta contamina las
  revisiones posteriores. Se descarta para v1.

## Consecuencias

- Un presupuesto de 10⁴ conceptos ocupa del orden de pocos MB por revisión:
  aceptable para una aplicación de escritorio.
- La comparación de revisiones (informe 13) es una consulta entre dos copias.
