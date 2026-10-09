# Caso de validación: suelo radiante

> **Datos sintéticos.** Precios orientativos inventados para pruebas; no
> proceden de clientes ni de tarifas reales. Implementado en
> `crates/ppto-core/src/ejemplos.rs` y verificado por
> `crates/ppto-core/tests/suelo_radiante.rs`. Reproducible con `ppto demo`.

**Revisión técnica:** ☐ pendiente — firma y fecha del revisor en la PR.

## 1. Recursos

| Código | Ud | Descripción | Precio |
|---|---|---|---:|
| MO.OF1F | h | Oficial 1ª instalador de calefacción | 24,50 |
| MO.AYUF | h | Ayudante instalador de calefacción | 21,00 |
| MT.PANEL | m² | Panel aislante de tetones EPS 30 mm | 9,50 |
| MT.TUBO16 | m | Tubo PE-RT 16×2 con barrera de oxígeno | 1,10 |
| MT.BANDA | m | Banda perimetral | 0,60 |
| MT.COL8 | ud | Colector de 8 vías | 185,00 |

## 2. Descompuestos

**SR.M2 — m² Suelo radiante**

| Recurso | Cantidad | Precio | Cálculo | Importe |
|---|---:|---:|---|---:|
| MT.PANEL | 1,050 | 9,50 | 9,975 → | 9,98 |
| MT.TUBO16 | 7,000 | 1,10 | | 7,70 |
| MT.BANDA | 0,400 | 0,60 | | 0,24 |
| MO.OF1F | 0,250 | 24,50 | 6,125 → | 6,13 |
| MO.AYUF | 0,250 | 21,00 | | 5,25 |
| %MA | 2,000 % | 29,30 | 0,586 → | 0,59 |
| **Precio** | | | | **29,89** |

**SR.COL8 — ud Colector**: 185,00 + 1,5 × 24,50 + 1,0 × 21,00 = **242,75**

## 3. Mediciones y PEM

| Línea | Uds | Long | Anch | Parcial |
|---|---:|---:|---:|---:|
| Planta baja | 1 | 12,50 | 8,00 | 100,00 |
| Planta primera | 1 | 12,50 | 8,00 | 100,00 |
| Baños | 2 | 3,20 | 2,50 | 16,00 |
| A deducir escalera | −1 | 2,00 | 3,00 | −6,00 |
| **Total** | | | | **210,00 m²** |

| Partida | Cantidad | Precio | Importe |
|---|---:|---:|---:|
| SR.M2 | 210,00 | 29,89 | 6.276,90 |
| SR.COL8 | 3 | 242,75 | 728,25 |
| **PEM** | | | **7.005,15** |

## 4. Explosión de recursos

| Recurso | Cantidad | Importe |
|---|---:|---:|
| MO.OF1F | 52,5 + 4,5 = 57,0 h | 1.396,50 |
| MO.AYUF | 52,5 + 3,0 = 55,5 h | 1.165,50 |
| MT.PANEL | 220,5 m² | 2.094,75 |
| MT.TUBO16 | 1.470 m | 1.617,00 |
| MT.BANDA | 84 m | 50,40 |
| MT.COL8 | 3 ud | 555,00 |
| %MA | 210 × 0,59 | 123,90 |
| **Total** | | **7.003,05** |

Descuadre frente al PEM: 7.005,15 − 7.003,05 = **2,10 €**, explicado por los
redondeos de línea: (9,98 − 9,975) × 210 + (6,13 − 6,125) × 210 = 1,05 + 1,05.

## 5. Subcontratación del montaje (solo mano de obra de SR.M2)

Coste propio retirado: 210 × 6,13 + 210 × 5,25 = 1.287,30 + 1.102,50 = **2.389,80 €**.
Horas liberadas: 52,5 h de oficial + 52,5 h de ayudante. Los colectores
mantienen su mano de obra propia.

| Oferta | Unidad | Cantidad | Precio | Contratado | €/m² equiv. | PEM escenario | Ahorro |
|---|---|---:|---:|---:|---:|---:|---:|
| SUB-A | m² | 210,00 | 9,00 | 1.890,00 | 9,00 | 6.505,35 | 499,80 |
| SUB-B | m de tubo (×7) | 1.470,00 | 1,60 | 2.352,00 | 11,20 | 6.967,35 | 37,80 |
| SUB-C | alzado | — | — | 2.300,00 | 10,95 | 6.915,35 | 89,80 |

Comprobaciones adicionales:
- Aplicar SUB-A y SUB-B a la vez es un error (doble sustitución de MO.OF1F).
- Subcontratar oficial y ayudante en dos paquetes distintos sí es válido.
- El PEM original sigue siendo 7.005,15 tras simular.
- Los medios auxiliares (2 %) se mantienen sobre la base original:
  **criterio a decidir en revisión técnica** (alternativa: recalcular sobre
  la base sin mano de obra, 0,36 €/m² en lugar de 0,59 €/m²).

## 6. Resumen con GG 13 %, BI 6 %, IVA 21 %

| | Importe |
|---|---:|
| PEM | 7.005,15 |
| GG 13 % (910,6695) | 910,67 |
| BI 6 % (420,309) | 420,31 |
| Base | 8.336,13 |
| IVA 21 % (1.750,5873) | 1.750,59 |
| **Total** | **10.086,72** |
