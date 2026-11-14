# ADR-0008: Legalización como aplicación independiente

- Estado: Propuesto
- Fecha: 2026-11-14

## Contexto

El cálculo de costes de legalización (tasas, EICI, visados y honorarios de
memorias y proyectos RITE/PCI) era una ventana dentro de `ppto-gui`. Se
quiere usar también **sin el presupuestador** (para un BC3 de Presto, o para
preparar una oferta de legalización suelta) y que crezca por su cuenta (más
disciplinas, más comunidades autónomas) sin tocar la interfaz principal.

## Decisión

- El motor sigue en `ppto-legalizacion` (biblioteca, sin interfaz).
- Nuevo crate `ppto-legalizacion-app` con el binario **`legalizacion`** y su
  propia ventana egui (el formulario que antes estaba en `ppto-gui`).
  - **Sola**: `legalizacion [obra.bc3]`. Abre un BC3, calcula, crea el
    capítulo `LEG` y guarda otro BC3.
  - **Lanzada**: `legalizacion --solicitud <json> --respuesta <json>`.
- «Legalización…» en `ppto-gui` escribe `solicitud.json` (obra + GG/BI) en
  una carpeta temporal, lanza `legalizacion` junto a `ppto-gui` y, cuando se
  cierra, lee `respuesta.json` (disciplina + `Calculo`) y aplica la partida
  con deshacer. Si se cierra sin crear partida, no cambia nada.
- El contrato son los dos JSON de `ppto_legalizacion::intercambio`, con
  `version`; las aplicaciones no comparten código de interfaz.
- Mismo repositorio y mismo instalador: los dos `.exe` van juntos y con la
  misma versión.

## Consecuencias

- La legalización se usa sola o desde la obra, con el mismo cálculo.
- El presupuestador sigue siendo usable mientras la otra ventana está
  abierta; la partida se aplica sobre la obra tal como esté al cerrarla.
- `cargo run -p ppto-gui` no compila `legalizacion`: en desarrollo hay que
  compilar también `-p ppto-legalizacion-app` (lo hacen la CI, la release y
  `abrir-interfaz.bat`).
- Un cambio incompatible en el intercambio exige subir `VERSION_INTERCAMBIO`.
