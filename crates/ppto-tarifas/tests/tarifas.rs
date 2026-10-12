// SPDX-License-Identifier: GPL-3.0-or-later
//! Tarifa de subcontrata sintética (datos inventados).

use ppto_core::{Naturaleza, Presupuesto};
use ppto_tarifas::{Biblioteca, deducir_titulo, deducir_unidad, leer_excel};
use rust_decimal_macros::dec;
use rust_xlsxwriter::Workbook;

fn excel(dir: &std::path::Path) -> std::path::PathBuf {
    let ruta = dir.join("tarifa.xlsx");
    let mut wb = Workbook::new();
    let h = wb.add_worksheet();
    h.write(0, 1, "TARIFA PRECIOS SUBCO EJEMPLO 2031").unwrap();
    h.write(1, 1, "DETECCION DE INCENDIOS").unwrap();
    for (c, t) in ["Referencia", "Descripcion", "Precio (€)"].iter().enumerate() {
        h.write(3, u16::try_from(c).unwrap(), *t).unwrap();
    }
    let filas: [(&str, &str, &str); 6] = [
        ("X001", "M.O. OFICIAL (1H)", "30"),
        ("X010", "INSTALACION DETECTOR OPTICO", "11,5"),
        ("X020", "INSTALACION CABLE ROJO 2x1,5mm / METRO", "3,25 €"),
        ("X021", "INSTALACION TUBO PVC RIGIDO", "1.5"),
        ("X030", "DESMONTAJE INSTALACION EXISTENTE", "1,80 €"),
        ("X040", "PROGRAMACION CENTRAL", "1.200,00 €"),
    ];
    for (i, (r, d, p)) in filas.iter().enumerate() {
        let f = u32::try_from(i).unwrap() + 4;
        h.write(f, 0, *r).unwrap();
        h.write(f, 1, *d).unwrap();
        // Uno como número de Excel, el resto como texto con €
        if *p == "30" {
            h.write(f, 2, 30.0).unwrap();
        } else {
            h.write(f, 2, *p).unwrap();
        }
    }
    h.write(11, 1, "Precios netos sin IVA").unwrap();
    wb.save(&ruta).unwrap();
    ruta
}

#[test]
fn lee_la_tarifa_y_deduce_unidades() {
    let dir = std::env::temp_dir().join(format!("tarifas-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let t = leer_excel(&excel(&dir), "", "").unwrap();
    assert_eq!((t.proveedor.as_str(), t.vigencia.as_str()), ("SUBCO EJEMPLO", "2031"));
    assert_eq!(t.especialidad, "DETECCION DE INCENDIOS");
    assert_eq!(t.lineas.len(), 6);
    let l = |r: &str| t.lineas.iter().find(|l| l.referencia == r).unwrap();
    assert_eq!((l("X001").unidad.as_str(), l("X001").precio), ("h", dec!(30)));
    assert_eq!((l("X010").unidad.as_str(), l("X010").precio), ("ud", dec!(11.5)));
    assert_eq!((l("X020").unidad.as_str(), l("X020").precio), ("m", dec!(3.25)));
    assert_eq!(l("X040").precio, dec!(1200));
    // Dudosas: tubo (m probable) y desmontaje
    assert!(l("X021").revisar_unidad && l("X021").unidad == "m");
    assert!(l("X030").revisar_unidad);
    assert_eq!(t.por_revisar(), 2);
    assert_eq!(t.codigo(l("X010")), "SUBCOEJE.X010");

    // Alta en un presupuesto como recurso de subcontrata
    let mut p = Presupuesto::new("P", "OBRA", "Obra");
    let (c, nuevo) = t.insertar_en(l("X010"), &mut p).unwrap();
    assert!(nuevo);
    let x = p.concepto(&c).unwrap();
    assert_eq!(
        (x.naturaleza, x.precio, x.unidad.as_str()),
        (Naturaleza::Subcontrata, dec!(11.5), "ud")
    );
    assert!(!t.insertar_en(l("X010"), &mut p).unwrap().1, "segunda vez reutiliza");

    // Biblioteca: guardar, cargar, buscar y sustituir la misma tarifa
    let ruta = dir.join("tarifas.json");
    let mut b = Biblioteca::default();
    assert!(!b.anadir(t.clone()));
    b.guardar(&ruta).unwrap();
    let mut b = Biblioteca::cargar(&ruta);
    assert_eq!(b.tarifas, vec![t.clone()]);
    let e = b.buscar("detector óptico");
    assert_eq!(e.len(), 1);
    assert_eq!(b.linea(e[0]).unwrap().1.referencia, "X010");
    assert_eq!(b.buscar("x02").len(), 2);
    assert_eq!(b.buscar("subco cable").len(), 1);
    assert!(b.anadir(t));
    assert_eq!(b.tarifas.len(), 1);
}

#[test]
fn unidades_y_titulos() {
    assert_eq!(deducir_unidad("M.O. 2 TECNICOS (1H)"), ("h", true));
    assert_eq!(
        deducir_unidad("INSTALACION MANGUERA RF, NARANJA 230V/ METRO"),
        ("m", true)
    );
    assert_eq!(deducir_unidad("INSTALACION CABLEADO UTP CAT 6"), ("m", false));
    assert_eq!(
        deducir_unidad("COMPROBACION DE CENTRAL Y PUESTA EN MARCHA"),
        ("ud", true)
    );
    assert_eq!(deducir_unidad("CERTIFICADO DE INSTALACION"), ("ud", true));
    assert_eq!(deducir_unidad("REVISION ANUAL"), ("ud", false));
    assert_eq!(
        deducir_titulo("TARIFA PRECIOS ACME 2030"),
        ("ACME".to_owned(), "2030".to_owned())
    );
}
