# ADR-0003: Aritmética decimal y redondeo comercial

- Estado: Propuesto
- Fecha: 2026-10-09
- Issue: P-003

## Contexto

En coma flotante, 2,675 se almacena como 2,67499999… y redondea a 2,67. Un
presupuesto con miles de líneas acumula diferencias visibles frente a Presto
y frente al cálculo manual de un revisor.

## Decisión

- `rust_decimal::Decimal` en todo cálculo económico; `f64` prohibido por lint.
- Redondeo comercial (mitad alejándose de cero) en una única función
  `ppto_core::redondear`.
- Criterio «lo que se ve es lo que se multiplica»: se redondea cada operando a
  sus decimales de presentación antes de operar (ver `docs/reglas-calculo.md`).
- Persistencia de importes como `TEXT` decimal en SQLite (nunca `REAL`).
- Decimales configurables por presupuesto, alineados con el registro `~K`.

## Consecuencias

- Resultados idénticos en todas las plataformas y reproducibles a mano.
- La explosión de recursos puede no cuadrar al céntimo con el PEM; la
  diferencia se calcula y se muestra (descuadre de redondeo).
- Formatear decimales en pantalla debe pasar por `redondear` (el `Display`
  con precisión de `rust_decimal` usa redondeo bancario).
