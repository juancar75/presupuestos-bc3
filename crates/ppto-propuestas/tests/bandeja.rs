// SPDX-License-Identifier: GPL-3.0-or-later
use ppto_core::ejemplos::suelo_radiante;
use ppto_core::propuesta::{Confianza, Contenido, EstadoPropuesta, Propuesta};
use ppto_propuestas::{Bandeja, ObraActiva, clave};
use rust_decimal_macros::dec;

fn prop(partida: &str, precio: rust_decimal::Decimal) -> Propuesta {
    Propuesta {
        id: 0,
        partida: partida.into(),
        contenido: Contenido::PrecioDirecto(precio),
        confianza: Confianza::Alta,
        justificacion: "x".into(),
        autor: "IA".into(),
        estado: EstadoPropuesta::Aceptada, // se ignora: entra pendiente
        descuentos: Default::default(),
    }
}

#[test]
fn bandeja_y_obra_activa() {
    let dir = std::env::temp_dir().join(format!("bandeja-{}", std::process::id()));
    let ruta = dir.join("propuestas.json");
    let _ = std::fs::remove_file(&ruta);
    let p = suelo_radiante();
    let obra = clave(&p);

    let a = Bandeja::modificar(&ruta, |b| b.anadir(&obra, prop("P1", dec!(10)))).unwrap();
    let b = Bandeja::modificar(&ruta, |b| b.anadir(&obra, prop("P2", dec!(20)))).unwrap();
    Bandeja::modificar(&ruta, |b| b.anadir("otra [X]", prop("P1", dec!(1)))).unwrap();
    assert_eq!((a, b), (1, 2));
    let bd = Bandeja::cargar(&ruta);
    assert_eq!(bd.pendientes(&obra).len(), 2);
    assert_eq!(bd.buscar(1).unwrap().estado, EstadoPropuesta::Pendiente);

    // Una nueva propuesta para la misma partida sustituye a la pendiente
    let c = Bandeja::modificar(&ruta, |b| b.anadir(&obra, prop("P1", dec!(11)))).unwrap();
    let bd = Bandeja::cargar(&ruta);
    assert_eq!(c, 4);
    assert!(bd.buscar(1).is_none());
    assert_eq!(bd.pendientes(&obra).len(), 2);

    // Aceptada ya no está pendiente y no se sustituye
    Bandeja::modificar(&ruta, |b| b.marcar(2, EstadoPropuesta::Aceptada)).unwrap();
    Bandeja::modificar(&ruta, |b| b.anadir(&obra, prop("P2", dec!(25)))).unwrap();
    let bd = Bandeja::cargar(&ruta);
    assert_eq!(bd.buscar(2).unwrap().estado, EstadoPropuesta::Aceptada);
    assert_eq!(bd.pendientes(&obra).len(), 2);

    // Obra activa
    let r = dir.join("obra-activa.json");
    ObraActiva::guardar(&p, &r).unwrap();
    let o = ObraActiva::cargar(&r).unwrap();
    assert_eq!(o.clave, obra);
    assert_eq!(o.presupuesto, p);
    assert!(ObraActiva::cargar(&dir.join("no.json")).is_err());
}

#[test]
fn modificaciones_concurrentes_no_se_pierden() {
    let dir = std::env::temp_dir().join(format!("ppto-band-conc-{}", std::process::id()));
    let ruta = dir.join("propuestas.json");
    let _ = std::fs::remove_dir_all(&dir);
    let hilos: Vec<_> = (0..8)
        .map(|h| {
            let ruta = ruta.clone();
            std::thread::spawn(move || {
                for i in 0..10 {
                    let p = prop(&format!("P{h}-{i}"), dec!(1));
                    ppto_propuestas::Bandeja::modificar(&ruta, |b| b.anadir("O [1]", p)).unwrap();
                }
            })
        })
        .collect();
    for h in hilos {
        h.join().unwrap();
    }
    assert_eq!(ppto_propuestas::Bandeja::cargar(&ruta).entradas.len(), 80);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn una_bandeja_danada_no_se_sobrescribe() {
    let dir = std::env::temp_dir().join(format!("ppto-band-d-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let ruta = dir.join("propuestas.json");
    std::fs::write(&ruta, "[roto").unwrap();
    assert!(ppto_propuestas::Bandeja::modificar(&ruta, |b| b.anadir("O", prop("X", dec!(1)))).is_err());
    assert_eq!(std::fs::read_to_string(&ruta).unwrap(), "[roto");
    let _ = std::fs::remove_dir_all(&dir);
}
