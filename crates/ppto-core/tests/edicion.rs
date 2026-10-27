// SPDX-License-Identifier: GPL-3.0-or-later
//! Edición estructural (P-014): crear, añadir, quitar, ordenar y borrar,
//! con cifras calculadas a mano.

use ppto_core::ejemplos::suelo_radiante;
use ppto_core::{Naturaleza, Presupuesto};
use rust_decimal_macros::dec;

#[test]
fn presupuesto_desde_cero() {
    // Obra nueva: C01 con una partida de 2 recursos y un 3 % de medios auxiliares.
    // P1 = 0,5 h × 20,00 + 2 m × 3,10 = 10,00 + 6,20 = 16,20; 3 % de 16,20 = 0,486 → 0,49
    // (Presto: 16,20/100 = 0,162 × 3 = 0,486 → 0,49). P1 = 16,69 €/m.
    // Medición 12 m → importe 200,28 €.
    let mut p = Presupuesto::new("Nueva", "OBRA", "Obra nueva");
    p.nuevo_capitulo("OBRA", "C01", "Fontanería").unwrap();
    p.nueva_partida("C01", "P1", "m", "Tubería PPR 20 mm").unwrap();
    p.nuevo_recurso("MO1", "h", "Oficial fontanero", Naturaleza::ManoObra, dec!(20))
        .unwrap();
    p.nuevo_recurso("MT1", "m", "Tubo PPR 20", Naturaleza::Material, dec!(3.10))
        .unwrap();
    p.nuevo_recurso("%MA", "%", "Medios auxiliares", Naturaleza::Porcentaje, dec!(0))
        .unwrap();
    p.anadir_linea("P1", "MO1", dec!(0.5)).unwrap();
    p.anadir_linea("P1", "MT1", dec!(2)).unwrap();
    p.anadir_linea("P1", "%MA", dec!(3)).unwrap();
    p.fijar_rendimiento("C01", "P1", dec!(12)).unwrap();
    assert_eq!(p.precio("P1").unwrap(), dec!(16.69));
    assert_eq!(p.pem().unwrap(), dec!(200.28));

    // Reordenar: %MA arriba del todo no tiene base → P1 = 16,20
    p.mover_linea("P1", "%MA", true).unwrap();
    p.mover_linea("P1", "%MA", true).unwrap();
    p.mover_linea("P1", "%MA", true).unwrap(); // ya es la primera: no hace nada
    assert_eq!(p.concepto("P1").unwrap().descomposicion[0].hijo, "%MA");
    assert_eq!(p.precio("P1").unwrap(), dec!(16.20));
    p.mover_linea("P1", "%MA", false).unwrap();
    p.mover_linea("P1", "%MA", false).unwrap();
    assert_eq!(p.precio("P1").unwrap(), dec!(16.69));
    assert_eq!(p.codigo_libre("C", 2), "C02");
}

#[test]
fn reglas_que_protegen_el_presupuesto() {
    let mut p = suelo_radiante();
    let antes = p.clone();
    let err = |r: Result<(), ppto_core::ErrorMotor>| r.unwrap_err().to_string();
    // Partida dentro de sí misma (directa o indirecta) → circular, y no se toca nada
    assert!(err(p.anadir_linea("SR.M2", "SR.M2", dec!(1))).contains("circular"));
    // Recurso en un capítulo, capítulo en una partida, recurso descompuesto
    assert!(err(p.anadir_linea("C01", "MO.OF1F", dec!(1))).contains("solo puede haber"));
    assert!(err(p.anadir_linea("SR.M2", "C01", dec!(1))).contains("no puede formar parte"));
    assert!(err(p.anadir_linea("MO.OF1F", "SR.M2", dec!(1))).contains("no se descompone"));
    // Repetido, código duplicado o con caracteres reservados de BC3
    assert!(err(p.anadir_linea("C01", "SR.M2", dec!(1))).contains("ya está"));
    assert!(err(p.nueva_partida("C01", "SR.M2", "m2", "x")).contains("duplicado"));
    assert!(err(p.nueva_partida("C01", "A|B", "m2", "x")).contains("reservado"));
    assert!(err(p.nueva_partida("C01", " ", "m2", "x")).contains("vacío"));
    assert!(err(p.nueva_partida("SR.M2", "NUEVA", "m2", "x")).contains("no es un capítulo"));
    // Borrar algo en uso o la raíz
    assert!(err(p.borrar_concepto("MO.OF1F")).contains("se usa en"));
    assert!(err(p.borrar_concepto(&p.raiz.clone())).contains("raíz"));
    assert_eq!(p, antes);
}

#[test]
fn quitar_y_purgar() {
    // Quitar SR.COL8 (3 × 242,75 = 728,25) de C01: PEM 7.005,15 → 6.276,90.
    let mut p = suelo_radiante();
    let total = p.conceptos.len();
    let borrados = p.quitar_y_purgar("C01", "SR.COL8").unwrap();
    assert_eq!(p.pem().unwrap(), dec!(6276.90));
    // Se borra la partida y los recursos que solo usaba ella; los compartidos quedan
    assert!(borrados.contains(&"SR.COL8".to_owned()));
    for b in &borrados {
        assert!(!p.conceptos.contains_key(b));
        assert!(p.padres(b).is_empty());
    }
    assert_eq!(p.conceptos.len(), total - borrados.len());
    assert!(p.conceptos.contains_key("MO.OF1F"), "lo usa SR.M2");
    assert!(!p.mediciones.contains_key(&("C01".into(), "SR.COL8".into())));
    p.validar().unwrap();

    // quitar_linea no borra el concepto: se puede volver a colgar
    let mut q = suelo_radiante();
    q.quitar_linea("C01", "SR.COL8").unwrap();
    assert!(q.conceptos.contains_key("SR.COL8"));
    q.anadir_linea("C01", "SR.COL8", dec!(3)).unwrap();
    assert_eq!(q.pem().unwrap(), dec!(7005.15));
}

#[test]
fn subpartidas_a_varios_niveles_como_presto() {
    // P1 (m) = 1 × AUX1 + 0,1 h × 20,00; AUX1 (ud) = 2 × AUX2 + 1 m × 3,10;
    // AUX2 (ud) = 0,5 h × 20,00 = 10,00 → AUX1 = 20,00 + 3,10 = 23,10 → P1 = 23,10 + 2,00 = 25,10.
    // 10 m → PEM 251,00 €.
    let mut p = Presupuesto::new("t", "OBRA", "Obra");
    p.nuevo_capitulo("OBRA", "C01", "Cap").unwrap();
    p.nueva_partida("C01", "P1", "m", "Partida").unwrap();
    p.nuevo_recurso("MO", "h", "Oficial", Naturaleza::ManoObra, dec!(20))
        .unwrap();
    p.nuevo_recurso("MT", "m", "Tubo", Naturaleza::Material, dec!(3.10))
        .unwrap();
    p.nuevo_recurso("AUX1", "ud", "Subpartida", Naturaleza::Partida, dec!(999))
        .unwrap();
    p.nuevo_recurso("AUX2", "ud", "Sub-subpartida", Naturaleza::Partida, dec!(0))
        .unwrap();
    p.anadir_linea("P1", "AUX1", dec!(1)).unwrap();
    p.anadir_linea("P1", "MO", dec!(0.1)).unwrap();
    p.anadir_linea("AUX1", "AUX2", dec!(2)).unwrap();
    p.anadir_linea("AUX1", "MT", dec!(1)).unwrap();
    p.anadir_linea("AUX2", "MO", dec!(0.5)).unwrap();
    p.fijar_rendimiento("C01", "P1", dec!(10)).unwrap();
    assert_eq!(p.precio("AUX2").unwrap(), dec!(10.00));
    assert_eq!(p.precio("AUX1").unwrap(), dec!(23.10));
    assert_eq!(p.precio("P1").unwrap(), dec!(25.10));
    assert_eq!(p.pem().unwrap(), dec!(251.00));
    assert_eq!(p.ruta("AUX2"), ["OBRA", "C01", "P1", "AUX1", "AUX2"]);
    // Ciclo indirecto: AUX2 no puede contener a P1
    assert!(
        p.anadir_linea("AUX2", "P1", dec!(1))
            .unwrap_err()
            .to_string()
            .contains("circular")
    );
    assert!(p.nuevo_recurso("X", "", "x", Naturaleza::Capitulo, dec!(0)).is_err());
}

#[test]
fn trasladar_partidas_entre_capitulos_como_arrastrar() {
    // Suelo radiante: C01 con SR.M2 (210 m², con medición) y SR.COL8 (3 ud).
    let mut p = suelo_radiante();
    p.nuevo_capitulo(&p.raiz.clone(), "C02", "Colectores").unwrap();
    // SR.M2 a C02: conserva cantidad y hoja de medición; el PEM no cambia
    p.trasladar_linea("C01", "SR.M2", "C02", None).unwrap();
    assert_eq!(p.pem().unwrap(), dec!(7005.15));
    assert!(p.mediciones.contains_key(&("C02".into(), "SR.M2".into())));
    assert!(!p.mediciones.contains_key(&("C01".into(), "SR.M2".into())));
    assert_eq!(p.lineas_valoradas("C02").unwrap()[0].importe, dec!(6276.90));
    // Volver a C01 delante de SR.COL8
    p.trasladar_linea("C02", "SR.M2", "C01", Some("SR.COL8")).unwrap();
    let orden: Vec<_> = p
        .concepto("C01")
        .unwrap()
        .descomposicion
        .iter()
        .map(|l| l.hijo.clone())
        .collect();
    assert_eq!(orden, ["SR.M2", "SR.COL8"]);
    // Reordenar dentro del mismo capítulo
    p.trasladar_linea("C01", "SR.COL8", "C01", Some("SR.M2")).unwrap();
    assert_eq!(p.concepto("C01").unwrap().descomposicion[0].hijo, "SR.COL8");
    // Capítulo dentro de otro capítulo (subcapítulo)
    p.trasladar_linea(&p.raiz.clone(), "C02", "C01", None).unwrap();
    assert_eq!(p.ruta("C02"), [p.raiz.clone(), "C01".into(), "C02".into()]);
    // Prohibido: capítulo dentro de sí mismo, partida en una partida que ya la contiene...
    let antes = p.clone();
    assert!(p.trasladar_linea("C01", "C02", "C02", None).is_err());
    assert!(
        p.trasladar_linea(&p.raiz.clone(), "C01", "C02", None)
            .unwrap_err()
            .to_string()
            .contains("circular")
    );
    assert_eq!(p, antes, "los intentos fallidos no tocan nada");
}

#[test]
fn copiar_partida_a_un_subcapitulo() {
    // Suelo radiante (PEM 7.005,15): SR.M2 = 210 m² × 29,89 = 6.276,90 con medición.
    let mut p = suelo_radiante();
    p.nuevo_capitulo("C01", "C01.01", "Planta baja").unwrap();
    p.copiar_linea("C01", "SR.M2", "C01.01").unwrap();
    // Misma partida en dos sitios: el PEM suma las dos líneas (7.005,15 + 6.276,90)
    assert_eq!(p.pem().unwrap(), dec!(13282.05));
    assert!(p.mediciones.contains_key(&("C01.01".into(), "SR.M2".into())));
    // Cantidad independiente en cada sitio: 100 m² × 29,89 = 2.989,00
    p.fijar_rendimiento("C01.01", "SR.M2", dec!(100)).unwrap();
    assert_eq!(p.lineas_valoradas("C01.01").unwrap()[0].importe, dec!(2989.00));
    assert_eq!(p.pem().unwrap(), dec!(9994.15));
    // No se puede pegar dos veces en el mismo capítulo; no toca nada
    let antes = p.clone();
    assert!(p.copiar_linea("C01", "SR.M2", "C01.01").is_err());
    assert_eq!(p, antes);
}

#[test]
fn copiar_conceptos_de_una_base() {
    // Base sintética: partida PB1 = 2 h × 20,00 (MO.B) + 1 ud × 5,00 (MT.B) = 45,00
    let mut base = Presupuesto::new("Base", "BASE", "Base");
    base.nuevo_capitulo("BASE", "CB", "Cap").unwrap();
    base.nueva_partida("CB", "PB1", "ud", "Partida de base").unwrap();
    base.nuevo_recurso("MO.B", "h", "Oficial", Naturaleza::ManoObra, dec!(20))
        .unwrap();
    base.nuevo_recurso("MT.B", "ud", "Pieza", Naturaleza::Material, dec!(5))
        .unwrap();
    base.nuevo_recurso("MO.C", "ud", "Colocación rejilla", Naturaleza::ManoObra, dec!(32.50))
        .unwrap();
    base.anadir_linea("PB1", "MO.B", dec!(2)).unwrap();
    base.anadir_linea("PB1", "MT.B", dec!(1)).unwrap();

    let mut p = suelo_radiante();
    let pem = p.pem().unwrap();
    // La partida trae su descompuesto; el capítulo de la base no se copia
    let (nuevos, ya) = p
        .copiar_conceptos(&base, &["PB1".into(), "MO.C".into(), "CB".into()])
        .unwrap();
    assert_eq!(nuevos, ["MO.B", "MO.C", "MT.B", "PB1"]);
    assert!(ya.is_empty());
    assert_eq!(p.precio("PB1").unwrap(), dec!(45.00));
    assert!(!p.conceptos.contains_key("CB"));
    // Sin colocar en el árbol: el PEM no cambia
    assert_eq!(p.pem().unwrap(), pem);
    // Lo que ya existe en la obra no se toca
    p.fijar_precio("MO.C", dec!(30)).unwrap();
    let (nuevos, ya) = p.copiar_conceptos(&base, &["MO.C".into()]).unwrap();
    assert!(nuevos.is_empty());
    assert_eq!(ya, ["MO.C"]);
    assert_eq!(p.precio("MO.C").unwrap(), dec!(30));
    // Un código que no está en la base: error y nada cambia
    let antes = p.clone();
    assert!(p.copiar_conceptos(&base, &["NO.EXISTE".into(), "PB1".into()]).is_err());
    assert_eq!(p, antes);
}

#[test]
fn partida_sin_descomponer_con_precio_directo() {
    // Presupuesto ciego: P1 (12 m) a 15,50 €/m directo = 186,00; al descomponerla manda el descompuesto
    let mut p = Presupuesto::new("t", "OBRA", "Obra");
    p.nuevo_capitulo("OBRA", "C01", "Cap").unwrap();
    p.nueva_partida("C01", "P1", "m", "Tubería").unwrap();
    p.fijar_rendimiento("C01", "P1", dec!(12)).unwrap();
    p.fijar_precio("P1", dec!(15.50)).unwrap();
    assert_eq!(p.precio("P1").unwrap(), dec!(15.50));
    assert_eq!(p.pem().unwrap(), dec!(186.00));
    // Con CI 10 %: 15,50 × 1,10 = 17,05 → 204,60
    p.costes_indirectos = dec!(10);
    assert_eq!(p.pem().unwrap(), dec!(204.60));
    p.costes_indirectos = dec!(0);
    p.nuevo_recurso("MO", "h", "Oficial", Naturaleza::ManoObra, dec!(20))
        .unwrap();
    p.anadir_linea("P1", "MO", dec!(0.5)).unwrap();
    assert_eq!(p.precio("P1").unwrap(), dec!(10.00));
    assert!(
        p.fijar_precio("P1", dec!(1)).is_err(),
        "descompuesta: su precio sale de las líneas"
    );
}

#[test]
fn traer_precios_de_otra_obra() {
    // Obra «ciego» con SR.M2 (210 m²) y SR.COL8 (3 ud) sin precio; se traen
    // del ejemplo: SR.M2 29,89 y SR.COL8 242,75 → PEM 7.005,15, igual que el original.
    let base = suelo_radiante();
    let mut p = Presupuesto::new("ciego", "OBRA", "Ciego");
    p.nuevo_capitulo("OBRA", "C01", "Cap").unwrap();
    p.nueva_partida("C01", "SR.M2", "m2", "Suelo radiante").unwrap();
    p.nueva_partida("C01", "SR.COL8", "ud", "Colector").unwrap();
    p.fijar_rendimiento("C01", "SR.M2", dec!(210)).unwrap();
    p.fijar_rendimiento("C01", "SR.COL8", dec!(3)).unwrap();
    assert_eq!(p.pem().unwrap(), dec!(0));
    let nuevos = p.traer_precio(&base, "SR.M2").unwrap();
    assert!(nuevos.contains(&"MO.OF1F".to_owned()) && nuevos.contains(&"%MA".to_owned()));
    p.traer_precio(&base, "SR.COL8").unwrap();
    assert_eq!(p.precio("SR.M2").unwrap(), dec!(29.89));
    assert_eq!(p.pem().unwrap(), dec!(7005.15));
    // Ya descompuesta: no se sustituye; inexistente en el origen: error claro
    assert!(
        p.traer_precio(&base, "SR.M2")
            .unwrap_err()
            .to_string()
            .contains("ya está descompuesta")
    );
    p.nueva_partida("C01", "OTRA", "ud", "x").unwrap();
    assert!(p.traer_precio(&base, "OTRA").is_err());
    // Precio directo en el origen (partida sin descomponer)
    let mut b2 = Presupuesto::new("b", "B", "B");
    b2.nuevo_capitulo("B", "C", "c").unwrap();
    b2.nueva_partida("C", "OTRA", "ud", "x").unwrap();
    b2.fijar_precio("OTRA", dec!(55.5)).unwrap();
    p.traer_precio(&b2, "OTRA").unwrap();
    assert_eq!(p.precio("OTRA").unwrap(), dec!(55.50));
}

#[test]
fn comparar_dos_revisiones() {
    use ppto_core::comparar::{Estado, comparar};
    // R1: el ejemplo (PEM 7.005,15). R2: 4 colectores (+242,75), sin SR.M2
    // (−6.276,90) y una partida nueva P1 de 2 ud × 100,00 = 200,00.
    let r1 = suelo_radiante();
    let mut r2 = r1.clone();
    r2.fijar_rendimiento("C01", "SR.COL8", dec!(4)).unwrap();
    r2.quitar_linea("C01", "SR.M2").unwrap();
    r2.nueva_partida("C01", "P1", "ud", "Partida nueva").unwrap();
    r2.fijar_rendimiento("C01", "P1", dec!(2)).unwrap();
    r2.fijar_precio("P1", dec!(100)).unwrap();
    let c = comparar(&r1, &r2).unwrap();
    assert_eq!(c.pem_antes, dec!(7005.15));
    // 7.005,15 + 242,75 − 6.276,90 + 200,00 = 1.171,00
    assert_eq!(c.pem_despues, dec!(1171.00));
    assert_eq!(c.diferencia(), dec!(-5834.15));
    assert_eq!(c.variacion_pct(), Some(dec!(-83.28)));
    let por = |cod: &str| c.cambios.iter().find(|x| x.codigo == cod).unwrap();
    assert_eq!(por("SR.COL8").estado, Estado::Cambiada);
    assert_eq!(por("SR.COL8").diferencia(), dec!(242.75));
    assert_eq!(por("SR.M2").estado, Estado::Quitada);
    assert_eq!(por("P1").estado, Estado::Nueva);
    assert_eq!(c.iguales, 0);
    // La suma de las diferencias de partidas es la diferencia del PEM
    let suma: rust_decimal::Decimal = c.cambios.iter().map(|x| x.diferencia()).sum();
    assert_eq!(suma, c.diferencia());
    // Sin cambios: nada que listar
    assert!(comparar(&r1, &r1).unwrap().cambios.is_empty());
}
