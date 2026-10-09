# Matriz de registros FIEBDC-3 (P-004)

> Estado: **borrador inicial**. Cada fila debe contrastarse con el texto
> oficial de la especificación (fiebdc.es) y con ficheros exportados por
> Presto 8.8. Lo marcado «a confirmar» procede de conocimiento previo y no
> está todavía verificado contra la norma.

## Alcance por fases

| Registro | Contenido | v0.2 importar | v0.2 exportar | Presto 8.8 | Modelo interno |
|---|---|:-:|:-:|---|---|
| `~V` | Propietario, versión del formato, programa, cabecera, juego de caracteres | ● | ● | a confirmar | metadatos de revisión |
| `~K` | Decimales, CI/GG/BI/baja/IVA, divisa | ● | ● | a confirmar | `Decimales` + resumen de venta |
| `~C` | Código, unidad, resumen, precio(s), fecha(s), tipo | ● | ● | a confirmar | `Concepto` |
| `~D` | Descomposición: hijo, factor, rendimiento | ● | ● | a confirmar | `LineaDescomposicion` |
| `~Y` | Añadir a una descomposición existente | ● | — | a confirmar | se fusiona en `~D` |
| `~T` | Texto largo del concepto | ● | ● | a confirmar | `Concepto::texto` |
| `~M` | Mediciones: posición, total, líneas (tipo, comentario, uds, long, anch, alt) | ● | ● | a confirmar | `Medicion` |
| `~N` | Añadir mediciones | ● | — | a confirmar | se fusiona en `~M` |
| `~P` | Descripción paramétrica | ○ | ○ | a confirmar | v0.3 |
| `~L` | Pliegos de condiciones | ○ | ○ | a confirmar | v0.3 |
| `~Q`, `~J` | Pliegos (variantes) | ○ | ○ | a confirmar | v0.3 |
| `~W` | Ámbitos geográficos | ○ | ○ | a confirmar | v0.3 |
| `~G` | Información gráfica | ○ | ○ | a confirmar | adjunto |
| `~E` | Entidades (fabricantes, proveedores) | ○ | ○ | a confirmar | proveedores (P-019) |
| `~O` | Relación comercial | ○ | ○ | a confirmar | proveedores (P-019) |
| `~X` | Información técnica | ○ | ○ | a confirmar | v0.3 |
| `~A` | Claves de tesauro | ○ | ○ | a confirmar | búsqueda |
| `~B` | Cambio de código | ● | — | a confirmar | renombrado en importación |
| `~F` | Documentos adjuntos | ○ | ○ | a confirmar | adjunto |
| `~R` | Residuos (versiones recientes) | ○ | ○ | a confirmar | v0.3 |

● fase M2 · ○ conservar sin interpretar (ida y vuelta literal) cuando sea posible.

## Cuestiones a resolver con la norma y con Presto 8.8

1. Correspondencia campo a campo de `~K` con `Decimales` (hay ámbitos
   distintos para medición, rendimiento, importe de línea y precio).
2. Conceptos porcentuales: convención del carácter `%` en el código, máscara
   de aplicación y si el porcentaje viaja como 2 o como 0,02.
3. Tipo de concepto para subcontratas (no existe tipo propio en todas las
   versiones): ¿tipo 0 + convención de código, o campo de la versión 2024?
4. Fórmulas en `~M`: variables admitidas y semántica de campos vacíos.
5. Codificación: ANSI/cp1252 frente a UTF-8 según `~V`; Presto 8.8 exporta en
   cp1252 (a confirmar con fichero real).
6. Longitudes máximas: código ≤ 20 caracteres en la norma; Presto 8.8 admite
   menos en algunos campos (a confirmar); resumen ≤ 64 caracteres según
   experiencia previa.
7. Separador decimal y precisión de números en el fichero.
8. Raíz `##` y capítulos `#`: reglas de reconstrucción del árbol.
9. Varios precios por concepto (`~C` con varias fechas/ámbitos).

## Banco de pruebas

Ficheros en `tests/bc3/` (M2), **todos sintéticos o con autorización escrita**:

| Fichero | Objetivo |
|---|---|
| `minimo.bc3` | `~V ~K ~C ~D` con un capítulo y una partida |
| `suelo-radiante.bc3` | Caso de validación completo con mediciones y `%` |
| `auxiliares.bc3` | Precios auxiliares anidados en 3 niveles |
| `ciclo.bc3` | Referencia circular (debe rechazarse con informe) |
| `cp1252-acentos.bc3` | Ñ, tildes, «ª», «º», «€» |
| `presto88-export-*.bc3` | Exportaciones reales de Presto 8.8 de los anteriores |

Criterio de aceptación de ida y vuelta: importar → exportar → importar debe
dar el mismo `Presupuesto` y el mismo PEM al céntimo.
