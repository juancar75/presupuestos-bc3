// SPDX-License-Identifier: GPL-3.0-or-later
//! IFC sintético escrito a mano (tests/datos): unidades en mm, propiedades
//! en español y en inglés, tipos, plantas, espacios y sistemas.

use ppto_modelo::{Clase, Opciones, a_presupuesto, ifc};
use rust_decimal_macros::dec;

fn modelo() -> ppto_modelo::Modelo {
    ifc::leer(include_bytes!("datos/instalacion-sintetica.ifc")).unwrap()
}

#[test]
fn lee_elementos_tipos_plantas_y_sistemas() {
    let m = modelo();
    assert_eq!(m.nombre, "Edificio de prueba");
    assert_eq!(m.elementos.len(), 9);
    assert_eq!(m.ignorados, 1, "el muro");
    let tubos: Vec<_> = m.elementos.iter().filter(|e| e.clase == Clase::Tuberia).collect();
    assert_eq!(tubos.len(), 4);
    let t = &tubos[0];
    assert_eq!(t.tipo, "Tubería PP-R");
    assert_eq!(t.medida, "Ø32");
    assert_eq!(t.longitud, Some(dec!(2.5)), "cantidad base en mm");
    assert_eq!(t.planta, "Planta baja");
    assert_eq!(t.sistema, "Agua fría");
    assert_eq!(tubos[1].longitud, Some(dec!(1.25)), "propiedad «Longitud»");
    assert_eq!(tubos[2].planta, "Planta 1");
    // Tramo genérico IFC2x3 con tipo de conducto
    let c = m.elementos.iter().find(|e| e.clase == Clase::Conducto).unwrap();
    assert_eq!(
        (c.medida.as_str(), c.longitud, c.sistema.as_str()),
        ("400x300", Some(dec!(4)), "Impulsión")
    );
    // Lavabo dentro de un espacio de la planta 1
    let l = m.elementos.iter().find(|e| e.clase == Clase::Sanitario).unwrap();
    assert_eq!((l.tipo.as_str(), l.planta.as_str()), ("Lavabo", "Planta 1"));
    let d = m.elementos.iter().find(|e| e.clase == Clase::TerminalAire).unwrap();
    assert_eq!(d.tipo, "Difusor:600x600");
    // El último tubo no tiene longitud: aviso
    assert!(m.avisos.iter().any(|a| a.contains("1 tramo(s) sin longitud")));
}

#[test]
fn presupuesto_del_ifc() {
    let m = modelo();
    let p = a_presupuesto(
        &m,
        Opciones {
            por_sistema: true,
            por_planta: true,
        },
    )
    .unwrap();
    p.validar().unwrap();
    let cap: Vec<String> = p
        .concepto("OBRA")
        .unwrap()
        .descomposicion
        .iter()
        .map(|l| p.concepto(&l.hijo).unwrap().resumen.clone())
        .collect();
    assert_eq!(cap, vec!["Sin sistema", "Agua fría", "Impulsión"]);
    let cant = p.cantidades_partidas().unwrap();
    let tubo = p.conceptos.values().find(|c| c.resumen == "Tubería PP-R Ø32").unwrap();
    assert_eq!(cant[&tubo.codigo], dec!(5));
    let codo = p.conceptos.values().find(|c| c.resumen == "Codo 90:PP-R Ø32").unwrap();
    assert_eq!(cant[&codo.codigo], dec!(2));
    assert_eq!(codo.unidad, "ud");
    // El presupuesto se exporta a BC3 sin problemas
    let bc3 = ppto_bc3::exportar(&p, &Default::default()).unwrap();
    assert!(!bc3.bytes.is_empty());
}

#[test]
fn errores() {
    assert!(ifc::leer(b"no es un ifc").is_err());
    assert!(ifc::leer(b"ISO-10303-21;\nDATA;\n#1=IFCWALL('a',$,'x'\nENDSEC;").is_err());
    let vacio = ifc::leer(b"ISO-10303-21;\nDATA;\nENDSEC;\nEND-ISO-10303-21;").unwrap();
    assert!(vacio.elementos.is_empty());
    assert!(!vacio.avisos.is_empty());
}
