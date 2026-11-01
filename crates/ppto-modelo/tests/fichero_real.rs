// SPDX-License-Identifier: GPL-3.0-or-later
//! Prueba manual con un modelo real (no se sube al repositorio):
//! `PPTO_MODELO=obra.rvt cargo test -p ppto-modelo --test fichero_real -- --nocapture`

#[test]
fn modelo_real_si_se_indica() {
    let Ok(ruta) = std::env::var("PPTO_MODELO") else {
        return;
    };
    let t = std::time::Instant::now();
    let m = ppto_modelo::leer(std::path::Path::new(&ruta)).unwrap();
    println!(
        "{} elementos, {} ignorados, {:?}",
        m.elementos.len(),
        m.ignorados,
        t.elapsed()
    );
    for (c, n, l) in m.por_clase() {
        println!("  {}: {n} · {l} m", c.capitulo());
    }
    for a in &m.avisos {
        println!("  aviso: {a}");
    }
    let p = ppto_modelo::a_presupuesto(
        &m,
        ppto_modelo::Opciones {
            por_sistema: true,
            por_planta: true,
        },
    )
    .unwrap();
    let cant = p.cantidades_partidas().unwrap();
    for (c, x) in &p.conceptos {
        if x.naturaleza == ppto_core::Naturaleza::Partida {
            println!(
                "  {c} {} {} {}",
                x.unidad,
                x.resumen,
                cant.get(c).copied().unwrap_or_default()
            );
        }
    }
}
