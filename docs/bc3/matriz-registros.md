# Matriz de registros FIEBDC-3 (P-004)

> Estado: **importador v0.2 implementado** (`crates/ppto-bc3`). Las
> interpretaciones marcadas «a validar» funcionan con los ficheros sintéticos
> pero deben contrastarse con el texto oficial de la especificación
> (fiebdc.es) y con exportaciones reales de Presto 8.8 (P-012).

## Estado por registro

| Registro | Contenido | Importar | Exportar | Modelo interno | Notas |
|---|---|:-:|:-:|---|---|
| `~V` | Propietario, versión, programa, rótulo, juego de caracteres | ✅ | P-011 | `Cabecera` | El juego de caracteres decide la decodificación |
| `~K` | Decimales, CI/GG/BI/baja/IVA, divisa | ◐ | P-011 | `Porcentajes` | Se leen CI/GG/BI/baja/IVA. **Los decimales no se aplican todavía** (se usan los del proyecto). Un decimal negativo significa «como máximo» |
| `~C` | Código, unidad, resumen, precio, fecha, tipo | ✅ | P-011 | `Concepto` | Solo el primer código sinónimo y el primer precio |
| `~D` | Descomposición: hijo, factor, rendimiento | ✅ | P-011 | `LineaDescomposicion` | Factor vacío = 1 |
| `~Y` | Añadir a una descomposición | ✅ | — | se fusiona | |
| `~T` | Texto largo | ✅ | P-011 | `Concepto::texto` | |
| `~M` | Mediciones | ✅ | P-011 | `Medicion` | Tipos 1, 2 y 3 (fórmula en el comentario) |
| `~N` | Añadir mediciones | ✅ | — | se fusiona | |
| `~L ~Q ~J` | Pliegos | ○ | ○ | — | Se cuentan y se informa |
| `~P` | Descripción paramétrica | ○ | ○ | — | |
| `~W ~G ~E ~O ~X ~A ~F ~R` | Ámbitos, gráficos, entidades, comercial, técnica, tesauro, adjuntos, residuos | ○ | ○ | — | |
| `~B` | Cambio de código | ○ | — | — | Pendiente |

✅ implementado · ◐ parcial · ○ no se importa (se informa con una incidencia «info»).

## Decisiones de interpretación del importador

| Tema | Decisión v0.2 | Estado |
|---|---|---|
| Codificación | `ANSI` o vacío → Windows-1252; `UTF-8` → UTF-8; `850`/`437` → Windows-1252 con aviso. Sin `~V` y con bytes UTF-8 válidos → UTF-8 | a validar con Presto 8.8 |
| Códigos | Se quitan espacios y `#` finales. `##` = raíz, `#` = capítulo | conforme a la norma |
| Naturaleza | Capítulo si `#`; porcentaje si el código contiene `%` o `&`; partida si tiene descomposición; si no, tipo `1` mano de obra, `2` maquinaria, `3` material, otro → «otros» | a validar |
| Porcentajes | Código con `%` (no acumulable) o `&` (acumulable). Rendimiento en **fracción** (`0.03` = 3 %). Máscara = prefijo del código antes del `%`/`&`: se aplica a las líneas anteriores cuyo código empieza por ella (vacía = todas). En precios compuestos `%` y `&` se calculan igual. Aviso si resulta > 100 % (exportador que escribe puntos) | conforme a FIEBDC-3; contrastar con Presto 8.8 |
| Rendimiento vacío | En capítulos se toma el total de `~M`; si no hay, 1 (con aviso fuera de capítulos) | a validar |
| Medición frente a `~D` | Se conserva la cantidad del `~D` (lo que exportó el programa) y se avisa si las líneas de `~M` suman otra cosa | decisión de diseño |
| Concepto usado y no definido | Se crea con precio 0 y se informa como error | decisión de diseño |
| Concepto repetido | Prevalece la última definición; aviso | a validar |
| Sin raíz `##` | Si hay un único capítulo suelto se toma como raíz; si no, se crea `RAIZ` con todos | decisión de diseño |
| Referencias circulares | Se elimina la última relación del ciclo y se informa como error | decisión de diseño |
| Números | Punto decimal; se acepta coma si no hay punto; notación científica admitida | conforme |

## Comprobación de precios («precios que no cuadran»)

Tras importar se recalcula todo con `ppto-core` y se compara el precio de
cada concepto con descomposición (y el total de cada capítulo) con el precio
declarado en su `~C`, redondeado a los decimales del proyecto. Las diferencias
se listan de mayor a menor. Sirve para:

1. detectar errores del presupuesto de origen;
2. detectar diferencias de criterio (redondeos, porcentajes, decimales de `~K`)
   con el programa que generó el BC3. **Es la prueba principal de P-012**: con
   un BC3 de Presto 8.8 correcto, la lista debe quedar vacía.

## Banco de pruebas

Ficheros en `crates/ppto-bc3/tests/datos/`, **todos sintéticos**:

| Fichero | Objetivo | Estado |
|---|---|---|
| `suelo-radiante.bc3` | Caso de validación completo en cp1252 con `~K`, `%`, `~T`, `~M` y `~L`: PEM 7.005,15 sin discrepancias | ✅ |
| `con-errores.bc3` | Un error de cada tipo, con su línea; el presupuesto se sigue valorando | ✅ |
| `utf8.bc3` | Juego de caracteres UTF-8 con ñ, ü, «», € | ✅ |
| (generado en la prueba) | 4.000 partidas, 491 KB: importación y recálculo en < 0,1 s (release) | ✅ |
| (generado en la prueba) | Truncado en cada byte y bytes alterados: nunca bloquea | ✅ |
| `presto88-export-*.bc3` | Exportaciones reales de Presto 8.8 (datos sintéticos o autorizados) | **pendiente: P-012** |

Criterio de aceptación de ida y vuelta (P-011): importar → exportar → importar
debe dar el mismo `Presupuesto` y el mismo PEM al céntimo.
