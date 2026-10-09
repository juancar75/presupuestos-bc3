# Reglas de cálculo económico (P-003)

> Estado: **propuesta para revisión técnica**. Cualquier cambio en este
> documento requiere la etiqueta `needs-engineering-review` y actualizar las
> pruebas que lo verifican (indicadas en cada apartado).

## 1. Números y redondeo

- Aritmética decimal exacta (28 dígitos significativos). No se usa coma
  flotante en ningún cálculo económico (lint `clippy::float_arithmetic = deny`).
- **Redondeo comercial**: mitad alejándose de cero. 0,125 → 0,13; −0,125 → −0,13;
  2,675 → 2,68. *No* se usa redondeo bancario.
  Prueba: `decimales::tests`.
- Decimales por ámbito (valores por defecto, configurables por presupuesto):

| Ámbito | Campo | Por defecto |
|---|---|---|
| Dimensiones de medición | `dimensiones` | 2 |
| Parciales y totales de medición | `medicion` | 2 |
| Rendimientos en descompuestos | `rendimiento` | 3 |
| Importe de línea de descompuesto | `importe_linea` | 2 |
| Precio de partida / auxiliar / recurso | `precio` | 2 |
| Importe de partida y total de capítulo | `importe` | 2 |

## 2. Precios descompuestos

Para cada línea del descompuesto de una partida o precio auxiliar:

```
cantidad = redondear(factor × rendimiento, rendimiento)
importe  = redondear(cantidad × precio_hijo, importe_linea)
precio_partida = redondear(Σ importes, precio)
```

- Los precios auxiliares (p. ej. mortero) se calculan igual y se usan como
  hijo en otras partidas. Prueba: `presupuesto::tests::precio_auxiliar_jerarquico_reutilizable`.
- **Líneas porcentuales** (naturaleza `Porcentaje`): la cantidad está en
  puntos porcentuales y se aplica sobre la suma de los importes de las líneas
  **anteriores** del mismo descompuesto cuyo código empieza por la
  **máscara** (prefijo del código antes de `%` o `&`; vacía = todas), como
  define FIEBDC-3: `importe = redondear(cantidad × base / 100, importe_linea)`.
  Ejemplo: `O%MA` 10 % sobre oficial 20,00 y material 100,00 → 2,00.
  Prueba: `porcentaje_con_mascara_solo_afecta_a_su_prefijo`.
- Se rechazan referencias circulares (A → B → … → A) indicando el ciclo, y
  los hijos inexistentes. Prueba: `detecta_referencia_circular`.

## 3. Mediciones

- Línea normal: producto de los campos rellenos (uds × long × anch × alt).
  Campo vacío = no interviene. Todos vacíos = 0. Uds negativas = deducción.
- Las dimensiones se redondean a `dimensiones` antes de multiplicar; cada
  parcial se redondea a `medicion`; el total es la suma de parciales
  redondeados.
- Fórmulas: `+ − × / ^` (exponente entero 0–10), paréntesis, variables
  `a b c d` = uds, long, anch, alt (vacío = 0) y `p` = π. `−a^2 = −(a²)`.
- Subtotales parciales y acumulados: informativos, no suman.
- Validación de unidades: aviso si se rellenan más dimensiones de las que
  admite la unidad (`ud`→0, `m`→1, `m2`→2, `m3`→3). Es aviso, no error.

Pruebas: `medicion::tests`.

## 4. Capítulos y PEM

```
cantidad_partida = redondear(medición total, medicion)
importe_partida  = redondear(cantidad_partida × precio_partida, importe)
total_capítulo   = redondear(Σ importes_partida, importe)
PEM              = total del capítulo raíz
```

## 5. Explosión de recursos

- Cantidad de cada recurso = Σ (cantidad de la partida en obra × cantidad de
  la línea), atravesando precios auxiliares.
- Importe del recurso = redondear(cantidad total × precio, importe).
- Las líneas porcentuales se acumulan como pseudo-recurso con su importe.
- **Descuadre de redondeo** = PEM − Σ importes de recursos. Se muestra
  siempre; no se reparte ni se oculta. En el caso de validación, 2,10 €.

## 6. Costes y venta (P-008)

| Concepto | Fórmula |
|---|---|
| Costes indirectos (K) | `precio = CD × (1 + K/100)` |
| Descuento sobre tarifa | `neto = tarifa × (1 − d/100)` |
| Recargo sobre coste | `venta = coste × (1 + r/100)` |
| Margen sobre venta | `venta = coste / (1 − m/100)`, con `m < 100` |
| Margen obtenido | `(venta − coste) / venta` |
| Recargo equivalente a un margen | `r = m / (1 − m)` |

Con 30 %: coste 100,00 → **142,86** por margen sobre venta, **130,00** por
recargo sobre coste (que solo da un 23,08 % de margen). Margen 30 % ≡ recargo
42,86 %.

Resumen de presupuesto: GG y BI se calculan sobre el PEM y se redondean por
separado; IVA sobre (PEM + GG + BI); el total es la suma de componentes
redondeados para que el documento impreso cuadre.

Pruebas: `venta::tests`.

## 7. Subcontratación de mano de obra (P-017)

- Un paquete asigna recursos concretos de partidas concretas.
- Unidad de contratación: por unidad (m², ml, ud…) con **factor de
  conversión** (unidades contratadas por unidad de partida) o alzado.
- Coste retirado por línea = redondear(cantidad de partida × importe de línea, importe).
- Cantidad contratada = redondear(cantidad de partida × factor, medicion);
  coste contratado = redondear(cantidad contratada × precio, importe).
- Alzado: se reparte entre asignaciones en proporción al coste retirado; el
  resto de redondeo va a la última asignación.
- Un mismo recurso de una misma partida no puede sustituirse dos veces.
- El presupuesto original no se modifica nunca.
- **Criterio provisional**: las líneas porcentuales del descompuesto original
  no se recalculan al retirar recursos; el escenario lo advierte en `notas`.
  *Decisión a validar en revisión técnica.*

Pruebas y caso completo: [`casos-validacion/suelo-radiante.md`](casos-validacion/suelo-radiante.md).
