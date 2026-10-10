# Matriz de registros FIEBDC-3 (P-004)

> Estado: **importador y exportador v0.2 implementados** (`crates/ppto-bc3`). Las
> interpretaciones marcadas «a validar» funcionan con los ficheros sintéticos
> pero deben contrastarse con el texto oficial de la especificación
> (fiebdc.es) y con exportaciones reales de Presto 8.8 (P-012).

## Estado por registro

| Registro | Contenido | Importar | Exportar | Modelo interno | Notas |
|---|---|:-:|:-:|---|---|
| `~V` | Propietario, versión, programa, rótulo, juego de caracteres | ✅ | ✅ | `Cabecera` | El juego de caracteres decide la decodificación |
| `~K` | Decimales, CI/GG/BI/baja/IVA, divisa | ◐ | ✅ | `Porcentajes` | Se leen CI/GG/BI/baja/IVA. **Los decimales no se aplican todavía** (se usan los del proyecto). Un decimal negativo significa «como máximo» |
| `~C` | Código, unidad, resumen, precio, fecha, tipo | ✅ | ✅ | `Concepto` | Solo el primer código sinónimo y el primer precio |
| `~D` | Descomposición: hijo, factor, rendimiento | ✅ | ✅ | `LineaDescomposicion` | Factor vacío = 1 |
| `~Y` | Añadir a una descomposición | ✅ | — | se fusiona | |
| `~T` | Texto largo | ✅ | ✅ | `Concepto::texto` | |
| `~M` | Mediciones | ✅ | ✅ | `Medicion` | Tipos 1, 2 y 3 (fórmula en el comentario) |
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

## Lo observado en exportaciones reales de Presto 8.8

Primer fichero real contrastado (09-10-2026, presupuesto pequeño de prueba;
no se sube al repositorio, se reproduce su estructura en
`tests/datos/presto88-minimo.bc3`). Resultado: 0 errores, 0 precios que no
cuadran, PEM idéntico.

| Aspecto | Presto 8.8 |
|---|---|
| `~V` | `SOFT S.A.|FIEBDC-3/2002|Presto 8.8||ANSI|` — versión sin fecha, rótulo vacío, **ANSI** |
| Fin de línea | CRLF |
| `~K` | `|\2\2\3\2\2\2\2\EUR\|0|` — decimales en formato antiguo (DN vacío, DD 2, DS 2, DR 3, DI 2, DP 2, DC 2, DM 2, divisa EUR) y **solo CI** en el segundo campo |
| `~C` | Precio y fecha `DDMMAA` en todos; tipo `0` en capítulos y partidas, `1` mano de obra, `3` material; unidad vacía si no se rellenó |
| `~E` | Presente aunque esté vacío |
| `~M` | **Se escribe aunque la partida no tenga líneas**: solo posición y total (`~M|01#\E01|1\1\|1||`). El importador no crea hoja de medición en ese caso |

Pendiente con un presupuesto real mayor: porcentajes (`%`), costes
indirectos ≠ 0, auxiliares, mediciones con líneas y textos.

## Exportador (P-011)

Escribe el formato de Presto 8.8: `FIEBDC-3/2002`, ANSI (Windows-1252), CRLF.

| Registro | Qué se escribe |
|---|---|
| `~V` | `presupuestos-bc3`, versión del programa, rótulo = nombre del presupuesto, `ANSI` |
| `~K` | Decimales del proyecto en el formato antiguo (`\2\2\3\2\2\2\2\EUR\` por defecto, igual que Presto) y `CI` (+ `GG\BI\0\IVA` si se indican) |
| `~C` | Raíz `##`, capítulos `#`; tipo 1/2/3 para mano de obra, maquinaria y material, 0 para el resto. Precio: recursos su precio; unidades de obra su **precio con indirectos**; auxiliares su coste; capítulos su total |
| `~D` | Hijos sin `#`; porcentajes en **fracción** (2 % → `0.02`) |
| `~T` | Texto largo con saltos CRLF |
| `~M` | `padre#\hijo`, total y líneas (tipo, comentario, uds, long, anch, alt; tipo 3 = fórmula) |

Saneado: `|`, `\` y `~` dentro de textos se sustituyen (`/`, `/`, `-`); los
caracteres sin equivalente en Windows-1252 se escriben como `?`. Ambos casos
se avisan. Las subcontratas salen con tipo 0 (FIEBDC-3/2002 no tiene tipo
propio) y también se avisa.

Pruebas de ida y vuelta (`tests/exportar.rs`): exportar → importar conserva
estructura, naturalezas, textos, mediciones, precios e importes al céntimo, y
exportar dos veces produce los mismos bytes. **Pendiente (P-012): abrir en
Presto 8.8 un BC3 exportado por el programa.**

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
| `presto88-minimo.bc3` | Estructura exacta de una exportación real de Presto 8.8 | ✅ |
| Presupuesto real mayor de Presto 8.8 | Porcentajes, CI, auxiliares, mediciones | **pendiente: P-012** |

Criterio de aceptación de ida y vuelta (P-011): importar → exportar → importar
debe dar el mismo `Presupuesto` y el mismo PEM al céntimo.
