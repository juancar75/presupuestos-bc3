// SPDX-License-Identifier: GPL-3.0-or-later
//! Edición desde la interfaz (P-014): acciones y pintado sin ventana.
use super::*;
use eframe::egui::{Context, RawInput};
use rust_decimal_macros::dec;

/// Pinta árbol, pestaña Partida y ventana de alta; devuelve las acciones pedidas.
fn pintar(app: &mut Aplicacion) -> Vec<Accion> {
    let ctx = Context::default();
    let mut acciones = Vec::new();
    for _ in 0..2 {
        let mut salida = ctx.run_ui(RawInput::default(), |ui| {
            egui::Panel::left("a").show(ui, |ui| app.arbol(ui, &mut acciones));
            egui::CentralPanel::default_margins().show(ui, |ui| app.pestana_partida(ui, &mut acciones));
            let c = ui.ctx().clone();
            app.ventana_alta(&c, &mut acciones);
        });
        salida.textures_delta.clear();
    }
    acciones
}

#[test]
fn presupuesto_nuevo_completo_desde_la_interfaz() {
    // Mismo caso que el motor: P1 = 0,5 × 20 + 2 × 3,10 + 3 % = 16,69; 12 m → 200,28 €
    let mut app = Aplicacion::sin_ventana();
    app.nuevo_presupuesto();
    assert_eq!(app.calc.pem, Decimal::ZERO);
    pintar(&mut app);
    app.aplicar(vec![Accion::NuevoCapitulo {
        padre: "OBRA".into(),
        codigo: "C01".into(),
        resumen: "Fontanería".into(),
    }]);
    app.aplicar(vec![Accion::NuevaPartida {
        capitulo: "C01".into(),
        codigo: "P1".into(),
        unidad: "m".into(),
        resumen: "Tubería".into(),
    }]);
    assert_eq!(app.sel, Some(("C01".into(), "P1".into())));
    pintar(&mut app);
    let recurso = |codigo: &str, n, precio, q| Accion::NuevoRecurso {
        codigo: codigo.into(),
        unidad: "u".into(),
        resumen: codigo.into(),
        naturaleza: n,
        precio,
        en: "P1".into(),
        cantidad: q,
    };
    app.aplicar(vec![
        recurso("MO1", Naturaleza::ManoObra, dec!(20), dec!(0.5)),
        recurso("MT1", Naturaleza::Material, dec!(3.10), dec!(2)),
        recurso("%MA", Naturaleza::Porcentaje, dec!(0), dec!(3)),
        Accion::Rendimiento("C01".into(), "P1".into(), dec!(12)),
    ]);
    assert_eq!(app.calc.error, None);
    assert_eq!(app.calc.pem, dec!(200.28));
    assert!(app.cambios);
    pintar(&mut app);

    // Un recurso con código ya usado no se cuelga por error en lugar del nuevo
    app.aplicar(vec![recurso("MO1", Naturaleza::Material, dec!(99), dec!(1))]);
    assert!(app.mensaje.as_ref().is_some_and(|(m, e)| *e && m.contains("duplicado")));
    assert_eq!(app.calc.pem, dec!(200.28));
    // Un recurso nuevo que no puede colgarse no se queda huérfano
    app.aplicar(vec![Accion::NuevoRecurso {
        codigo: "MO9".into(),
        unidad: "h".into(),
        resumen: "x".into(),
        naturaleza: Naturaleza::ManoObra,
        precio: dec!(1),
        en: "C01".into(),
        cantidad: dec!(1),
    }]);
    assert!(!app.p.conceptos.contains_key("MO9"));

    // Quitar la partida del árbol: se borra con todo lo que solo usaba ella
    app.aplicar(vec![Accion::QuitarLinea("C01".into(), "P1".into(), true)]);
    assert_eq!(app.calc.pem, Decimal::ZERO);
    assert_eq!(app.sel, None);
    assert!(!app.p.conceptos.contains_key("MO1"));
    pintar(&mut app);
}

#[test]
fn ventana_de_alta_y_lineas_del_descompuesto() {
    let mut app = Aplicacion::sin_ventana();
    app.aplicar(vec![Accion::Alta(TipoAlta::Linea { padre: "SR.M2".into() })]);
    assert!(app.alta.as_ref().is_some_and(|a| a.codigo == "R0001"));
    pintar(&mut app);
    assert!(app.alta.is_some(), "la ventana sigue abierta hasta confirmar");
    // Quitar y volver a poner el colector cambia el PEM y lo recupera
    app.aplicar(vec![Accion::QuitarLinea("C01".into(), "SR.COL8".into(), false)]);
    assert_eq!(app.calc.pem, dec!(6276.90));
    app.aplicar(vec![Accion::AnadirLinea("C01".into(), "SR.COL8".into(), dec!(3))]);
    assert_eq!(app.calc.pem, dec!(7005.15));
    // Mover y cambiar unidad
    let antes: Vec<_> = app.p.conceptos["SR.M2"]
        .descomposicion
        .iter()
        .map(|l| l.hijo.clone())
        .collect();
    app.aplicar(vec![Accion::Mover("SR.M2".into(), antes[1].clone(), true)]);
    assert_eq!(app.p.conceptos["SR.M2"].descomposicion[0].hijo, antes[1]);
    app.aplicar(vec![Accion::Unidad("SR.M2".into(), " m² ".into())]);
    assert_eq!(app.p.conceptos["SR.M2"].unidad, "m²");
    pintar(&mut app);
}

#[test]
fn subpartidas_desde_la_interfaz() {
    // Mismo caso que el motor: AUX2 10,00 → AUX1 23,10 → P1 25,10; 10 m → 251,00 €
    let mut app = Aplicacion::sin_ventana();
    app.nuevo_presupuesto();
    app.aplicar(vec![
        Accion::NuevoCapitulo {
            padre: "OBRA".into(),
            codigo: "C01".into(),
            resumen: "Cap".into(),
        },
        Accion::NuevaPartida {
            capitulo: "C01".into(),
            codigo: "P1".into(),
            unidad: "m".into(),
            resumen: "Partida".into(),
        },
    ]);
    let nuevo = |codigo: &str, n, precio, en: &str, q| Accion::NuevoRecurso {
        codigo: codigo.into(),
        unidad: "ud".into(),
        resumen: codigo.into(),
        naturaleza: n,
        precio,
        en: en.into(),
        cantidad: q,
    };
    app.aplicar(vec![
        nuevo("AUX1", Naturaleza::Partida, dec!(0), "P1", dec!(1)),
        nuevo("MO", Naturaleza::ManoObra, dec!(20), "P1", dec!(0.1)),
    ]);
    // Entrar en la subpartida (clic en su código) y descomponerla
    app.aplicar(vec![Accion::Seleccionar("P1".into(), "AUX1".into())]);
    pintar(&mut app);
    app.aplicar(vec![
        nuevo("AUX2", Naturaleza::Partida, dec!(0), "AUX1", dec!(2)),
        nuevo("MT", Naturaleza::Material, dec!(3.10), "AUX1", dec!(1)),
        Accion::Seleccionar("AUX1".into(), "AUX2".into()),
        Accion::AnadirLinea("AUX2".into(), "MO".into(), dec!(0.5)),
        Accion::Rendimiento("C01".into(), "P1".into(), dec!(10)),
    ]);
    pintar(&mut app);
    assert_eq!(app.calc.error, None);
    assert_eq!(app.calc.precios["AUX1"], dec!(23.10));
    assert_eq!(app.calc.pem, dec!(251.00));
    assert_eq!(app.p.ruta("AUX2"), ["OBRA", "C01", "P1", "AUX1", "AUX2"]);
}

#[test]
fn arrastrar_partida_a_otro_capitulo() {
    let mut app = Aplicacion::sin_ventana();
    app.aplicar(vec![Accion::NuevoCapitulo {
        padre: app.p.raiz.clone(),
        codigo: "C02".into(),
        resumen: "Otro".into(),
    }]);
    app.aplicar(vec![Accion::Trasladar(
        "C01".into(),
        "SR.M2".into(),
        "C02".into(),
        None,
    )]);
    assert_eq!(app.calc.pem, dec!(7005.15));
    assert_eq!(
        app.sel,
        Some(("C02".into(), "SR.M2".into())),
        "la selección sigue a la partida"
    );
    assert_eq!(app.calc.lineas["C02"][0].importe, dec!(6276.90));
    pintar(&mut app);
}

#[test]
fn subcapitulos_con_codigo_bajo_su_padre() {
    let mut app = Aplicacion::sin_ventana();
    app.aplicar(vec![Accion::Alta(TipoAlta::Capitulo {
        padre: app.p.raiz.clone(),
    })]);
    assert_eq!(app.alta.as_ref().unwrap().codigo, "C02");
    app.aplicar(vec![Accion::Alta(TipoAlta::Capitulo { padre: "C01".into() })]);
    assert_eq!(app.alta.as_ref().unwrap().codigo, "C01.01");
    pintar(&mut app);
}

#[test]
fn precio_directo_de_partida_sin_descomponer() {
    // Como al valorar un ciego: 12 m a 15,50 €/m = 186,00
    let mut app = Aplicacion::sin_ventana();
    app.nuevo_presupuesto();
    app.aplicar(vec![
        Accion::NuevoCapitulo {
            padre: "OBRA".into(),
            codigo: "C01".into(),
            resumen: "Cap".into(),
        },
        Accion::NuevaPartida {
            capitulo: "C01".into(),
            codigo: "P1".into(),
            unidad: "m".into(),
            resumen: "Tubería".into(),
        },
        Accion::Rendimiento("C01".into(), "P1".into(), dec!(12)),
        Accion::Precio("P1".into(), dec!(15.50)),
    ]);
    assert_eq!(app.calc.pem, dec!(186.00));
    pintar(&mut app);
}

#[test]
fn navegar_entre_partidas() {
    // Ejemplo: SR.M2 y SR.COL8 en C01; se añade C02 con P9 sin precio
    let mut app = Aplicacion::sin_ventana();
    app.aplicar(vec![
        Accion::NuevoCapitulo {
            padre: app.p.raiz.clone(),
            codigo: "C02".into(),
            resumen: "Otro".into(),
        },
        Accion::NuevaPartida {
            capitulo: "C02".into(),
            codigo: "P9".into(),
            unidad: "u".into(),
            resumen: "Sin precio".into(),
        },
    ]);
    assert_eq!(
        app.orden_partidas(),
        [
            ("C01".to_owned(), "SR.M2".to_owned()),
            ("C01".into(), "SR.COL8".into()),
            ("C02".into(), "P9".into())
        ]
    );
    app.aplicar(vec![Accion::Seleccionar("C01".into(), "SR.M2".into())]);
    app.aplicar(vec![Accion::Navegar(1)]);
    assert_eq!(app.sel, Some(("C01".into(), "SR.COL8".into())));
    app.aplicar(vec![Accion::Navegar(5)]);
    assert_eq!(app.sel, Some(("C02".into(), "P9".into())), "no se pasa del final");
    app.aplicar(vec![Accion::Navegar(-1)]);
    assert_eq!(app.sel, Some(("C01".into(), "SR.COL8".into())));
    app.aplicar(vec![Accion::SiguienteSinPrecio]);
    assert_eq!(app.sel, Some(("C02".into(), "P9".into())));
    assert!(app.desplazar_arbol);
    pintar(&mut app);
    app.buscar = "colector".into();
    pintar(&mut app);
}

/// Clic real con el ratón en una partida del árbol (regresión: el
/// arrastre no debe anular el clic).
#[test]
fn clic_en_partida_del_arbol() {
    use eframe::egui::{Event, PointerButton, Pos2};
    let mut elegidas = std::collections::BTreeSet::new();
    for y in (60..420).step_by(6) {
        let mut app = Aplicacion::sin_ventana();
        app.sel = None;
        let ctx = Context::default();
        let mut acciones = Vec::new();
        let pos = Pos2::new(90.0, y as f32);
        let boton = |pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        for e in [
            vec![],
            vec![Event::PointerMoved(pos)],
            vec![boton(true)],
            vec![boton(false)],
            vec![],
        ] {
            let entrada = RawInput {
                events: e,
                screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 800.0))),
                ..Default::default()
            };
            let mut out = ctx.run_ui(entrada, |ui| {
                egui::Panel::left("a")
                    .default_size(330.0)
                    .show(ui, |ui| app.arbol(ui, &mut acciones));
            });
            out.textures_delta.clear();
        }
        for a in acciones {
            if let Accion::Seleccionar(_, h) = a {
                elegidas.insert(h);
            }
        }
    }
    assert_eq!(elegidas.into_iter().collect::<Vec<_>>(), ["SR.COL8", "SR.M2"]);
}

/// Arrastrar con el ratón SR.COL8 encima de SR.M2 la coloca delante.
/// Arrastrar SR.COL8 desde `desde` hasta `hasta` en un árbol con un
/// subcapítulo vacío C01.01; devuelve las acciones.
fn arrastrar_a_subcapitulo(desde: f32, hasta: f32) -> Vec<Accion> {
    use eframe::egui::{Event, PointerButton, Pos2};
    let mut app = Aplicacion::sin_ventana();
    app.p.nuevo_capitulo("C01", "C01.01", "Planta baja").unwrap();
    let raiz = app.p.raiz.clone();
    app.p.nuevo_capitulo(&raiz, "C02", "Colectores").unwrap();
    app.recalcular();
    let ctx = Context::default();
    let mut acc = Vec::new();
    let boton = |y, pressed| Event::PointerButton {
        pos: Pos2::new(90.0, y),
        button: PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    let mut eventos = vec![
        vec![],
        vec![Event::PointerMoved(Pos2::new(90.0, desde))],
        vec![boton(desde, true)],
    ];
    for k in 1..=10 {
        eventos.push(vec![Event::PointerMoved(Pos2::new(
            90.0,
            desde + (hasta - desde) * k as f32 / 10.0,
        ))]);
    }
    eventos.push(vec![boton(hasta, false)]);
    eventos.push(vec![]);
    for e in eventos {
        let entrada = RawInput {
            events: e,
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 800.0))),
            ..Default::default()
        };
        let mut out = ctx.run_ui(entrada, |ui| {
            egui::Panel::left("a")
                .default_size(330.0)
                .show(ui, |ui| app.arbol(ui, &mut acc));
        });
        out.textures_delta.clear();
    }
    acc
}

#[test]
fn cortar_y_copiar_a_un_subcapitulo() {
    let mut app = Aplicacion::sin_ventana();
    app.p.nuevo_capitulo("C01", "C01.01", "Planta baja").unwrap();
    app.recalcular();
    // Copiar SR.COL8 (3 ud × 242,75 = 728,25) a C01.01: el PEM pasa de 7.005,15 a 7.733,40
    app.aplicar(vec![
        Accion::Portapapeles(Some(("C01".into(), "SR.COL8".into(), false))),
        Accion::Copiar("C01".into(), "SR.COL8".into(), "C01.01".into()),
    ]);
    assert_eq!(app.p.pem().unwrap(), dec!(7733.40));
    assert!(app.portapapeles.is_some(), "copiar deja la partida para pegar otra vez");
    // Cortar SR.M2 y pegar en C01.01: el PEM no cambia y se vacía el portapapeles
    app.aplicar(vec![
        Accion::Portapapeles(Some(("C01".into(), "SR.M2".into(), true))),
        Accion::Trasladar("C01".into(), "SR.M2".into(), "C01.01".into(), None),
    ]);
    assert_eq!(app.p.pem().unwrap(), dec!(7733.40));
    assert!(app.p.mediciones.contains_key(&("C01.01".into(), "SR.M2".into())));
    assert!(app.portapapeles.is_none());
}

#[test]
fn arrastrar_partida_a_un_subcapitulo() {
    // Buscar la y de SR.COL8 (la que, al soltar sin moverse, no traslada nada)
    // y comprobar que alguna posición de soltar la lleva a C01.01.
    let ys: Vec<f32> = (40..500).step_by(4).map(|y| y as f32).collect();
    let desde = ys.iter().copied().find(|&y| {
        arrastrar_a_subcapitulo(y, y + 1.0)
            .iter()
            .any(|a| matches!(a, Accion::Seleccionar(_, h) if h == "SR.COL8"))
    });
    let desde = desde.expect("no se encontró SR.COL8 en el árbol");
    let llega = ys.iter().any(|&h| {
        arrastrar_a_subcapitulo(desde, h)
            .iter()
            .any(|a| matches!(a, Accion::Trasladar(o, x, d, _) if o == "C01" && x == "SR.COL8" && d == "C01.01"))
    });
    assert!(llega, "no se puede soltar una partida en un subcapítulo vacío");
}

/// Clics sucesivos en el árbol (x = 90) con la misma aplicación.
fn clics(ys: &[f32]) -> Vec<Accion> {
    use eframe::egui::{Event, PointerButton, Pos2};
    let mut app = Aplicacion::sin_ventana();
    let ctx = Context::default();
    let mut acc = Vec::new();
    let mut eventos = vec![vec![]];
    for &y in ys {
        let p = Pos2::new(90.0, y);
        let b = |pressed| Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        eventos.extend([
            vec![Event::PointerMoved(p)],
            vec![b(true)],
            vec![b(false)],
            vec![],
            vec![],
        ]);
    }
    for e in eventos {
        let entrada = RawInput {
            events: e,
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 800.0))),
            ..Default::default()
        };
        let mut out = ctx.run_ui(entrada, |ui| {
            egui::Panel::left("a")
                .default_size(330.0)
                .show(ui, |ui| app.arbol(ui, &mut acc));
        });
        out.textures_delta.clear();
    }
    acc
}

#[test]
fn clic_en_el_titulo_contrae_el_capitulo() {
    let elige = |acc: &[Accion]| {
        acc.iter()
            .any(|a| matches!(a, Accion::Seleccionar(_, h) if h == "SR.COL8"))
    };
    let ys: Vec<f32> = (40..500).step_by(4).map(|y| y as f32).collect();
    let y_col8 = ys.iter().copied().find(|&y| elige(&clics(&[y]))).unwrap();
    // Algún clic por encima (el título de C01) oculta SR.COL8, y un segundo
    // clic en el mismo sitio la vuelve a mostrar.
    let titulo = ys
        .iter()
        .copied()
        .filter(|&y| y < y_col8 - 20.0)
        .find(|&y| !elige(&clics(&[y, y_col8])));
    let titulo = titulo.expect("el clic en el título no contrae el capítulo");
    assert!(
        elige(&clics(&[titulo, titulo, y_col8])),
        "un segundo clic debe volver a abrirlo"
    );
}

#[test]
fn poner_titulo_a_un_subcapitulo_pinchando_en_el_campo() {
    use eframe::egui::{Event, Key, PointerButton, Pos2};
    // Capítulo con título (el campo no recibe el foco solo): clic derecho,
    // clic en el campo, borrar, escribir y Enter.
    let mut hecho = false;
    for y in (40..200).step_by(4) {
        let mut app = Aplicacion::sin_ventana();
        app.p.nuevo_capitulo("C01", "C01.01", "Viejo").unwrap();
        app.recalcular();
        let ctx = Context::default();
        let mut acc = Vec::new();
        let frame = |e: Vec<Event>, app: &mut Aplicacion, acc: &mut Vec<Accion>| {
            let entrada = RawInput {
                events: e,
                screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 800.0))),
                ..Default::default()
            };
            let mut out = ctx.run_ui(entrada, |ui| {
                egui::Panel::left("a")
                    .default_size(330.0)
                    .show(ui, |ui| app.arbol(ui, acc));
            });
            out.textures_delta.clear();
        };
        let boton = |pos, button, pressed| Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: Default::default(),
        };
        let p = Pos2::new(60.0, y as f32);
        frame(vec![], &mut app, &mut acc);
        frame(vec![Event::PointerMoved(p)], &mut app, &mut acc);
        frame(vec![boton(p, PointerButton::Secondary, true)], &mut app, &mut acc);
        frame(vec![boton(p, PointerButton::Secondary, false)], &mut app, &mut acc);
        frame(vec![], &mut app, &mut acc);
        let Some(r) = ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new(CAMPO_TITULO))) else {
            continue;
        };
        // Mover hasta el campo y pinchar (pulsar y soltar en fotogramas distintos)
        let q = r.center();
        for k in 1..=5 {
            frame(
                vec![Event::PointerMoved(p + (q - p) * k as f32 / 5.0)],
                &mut app,
                &mut acc,
            );
        }
        frame(vec![boton(q, PointerButton::Primary, true)], &mut app, &mut acc);
        frame(vec![], &mut app, &mut acc);
        frame(vec![boton(q, PointerButton::Primary, false)], &mut app, &mut acc);
        frame(vec![], &mut app, &mut acc);
        // ¿Sigue dibujándose el menú? (la marca se pone en cada fotograma)
        ctx.data_mut(|d| d.remove::<String>(egui::Id::new(CAMPO_TITULO).with("abierto")));
        frame(vec![], &mut app, &mut acc);
        let cap = ctx.data(|d| d.get_temp::<String>(egui::Id::new(CAMPO_TITULO).with("abierto")));
        let cap = cap.expect("el menú se cerró al pinchar en el título");
        let tecla = |key, pressed| Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Default::default(),
        };
        let mut borrar = Vec::new();
        for _ in 0..6 {
            borrar.extend([tecla(Key::Backspace, true), tecla(Key::Backspace, false)]);
        }
        eprintln!("foco={:?}", ctx.memory(|m| m.focused()));
        frame(borrar, &mut app, &mut acc);
        eprintln!(
            "abierto tras borrar={:?}",
            ctx.data(|d| d.get_temp::<String>(egui::Id::new(CAMPO_TITULO).with("abierto")))
        );
        frame(vec![Event::Text("Planta baja".into())], &mut app, &mut acc);
        frame(
            vec![tecla(Key::Enter, true), tecla(Key::Enter, false)],
            &mut app,
            &mut acc,
        );
        frame(vec![], &mut app, &mut acc);
        hecho = acc
            .iter()
            .any(|a| matches!(a, Accion::Resumen(c, t) if *c == cap && t.contains("Planta baja")));
        assert!(hecho, "no se guardó el título de {cap}");
        break;
    }
    assert!(hecho, "no se encontró ningún capítulo");
}

#[test]
fn soltar_en_el_espacio_libre_lleva_al_nivel_principal() {
    // El subcapítulo C01.01 arrastrado al hueco de debajo del árbol pasa a la raíz
    let raiz = Aplicacion::sin_ventana().p.raiz;
    let llega = (40..400).step_by(4).any(|y| {
        arrastrar_a_subcapitulo(y as f32, 760.0)
            .iter()
            .any(|a| matches!(a, Accion::Trasladar(o, h, d, None) if o == "C01" && h == "C01.01" && *d == raiz))
    });
    assert!(llega, "no se puede llevar un subcapítulo al nivel principal");
}

#[test]
fn arrastrar_un_capitulo_dentro_de_otro() {
    let ys: Vec<f32> = (40..500).step_by(4).map(|y| y as f32).collect();
    let a_sub = |acc: &[Accion], h: &str| {
        acc.iter()
            .any(|a| matches!(a, Accion::Trasladar(_, x, d, _) if x == h && d == "C01.01"))
    };
    let desde = ys
        .iter()
        .copied()
        .find(|&y| {
            arrastrar_a_subcapitulo(y, y + 1.0)
                .iter()
                .any(|a| matches!(a, Accion::Seleccionar(_, h) if h == "SR.COL8"))
        })
        .unwrap();
    let destino = ys
        .iter()
        .copied()
        .find(|&h| a_sub(&arrastrar_a_subcapitulo(desde, h), "SR.COL8"))
        .unwrap();
    // El título de C02 (capítulo de la raíz) arrastrado al de C01.01
    let llega = ys.iter().any(|&y| {
        let acc = arrastrar_a_subcapitulo(y, destino);
        acc.iter()
            .any(|a| matches!(a, Accion::Trasladar(o, x, d, _) if *o == Aplicacion::sin_ventana().p.raiz && x == "C02" && d == "C01.01"))
    });
    assert!(llega, "no se puede arrastrar un capítulo a otro");
}

#[test]
fn arrastre_con_raton() {
    use eframe::egui::{Event, PointerButton, Pos2};
    // Posiciones de las dos partidas (clic de prueba en cada y)
    let mut pos_de = std::collections::BTreeMap::new();
    for y in (60..420).step_by(6) {
        let mut app = Aplicacion::sin_ventana();
        let ctx = Context::default();
        let mut acc = Vec::new();
        let p = Pos2::new(90.0, y as f32);
        let b = |pressed| Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        for e in [vec![], vec![Event::PointerMoved(p)], vec![b(true)], vec![b(false)]] {
            let entrada = RawInput {
                events: e,
                screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 800.0))),
                ..Default::default()
            };
            let mut out = ctx.run_ui(entrada, |ui| {
                egui::Panel::left("a")
                    .default_size(330.0)
                    .show(ui, |ui| app.arbol(ui, &mut acc));
            });
            out.textures_delta.clear();
        }
        for a in acc {
            if let Accion::Seleccionar(_, h) = a {
                pos_de.entry(h).or_insert(y as f32);
            }
        }
    }
    let (desde, hasta) = (pos_de["SR.COL8"], pos_de["SR.M2"]);
    let mut app = Aplicacion::sin_ventana();
    let ctx = Context::default();
    let mut acc = Vec::new();
    let p0 = Pos2::new(90.0, desde);
    let mut eventos = vec![
        vec![],
        vec![Event::PointerMoved(p0)],
        vec![Event::PointerButton {
            pos: p0,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        }],
    ];
    for k in 1..=10 {
        eventos.push(vec![Event::PointerMoved(Pos2::new(
            90.0,
            desde + (hasta - desde) * k as f32 / 10.0,
        ))]);
    }
    let p1 = Pos2::new(90.0, hasta);
    eventos.push(vec![Event::PointerButton {
        pos: p1,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    }]);
    for e in eventos {
        let entrada = RawInput {
            events: e,
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 800.0))),
            ..Default::default()
        };
        let mut out = ctx.run_ui(entrada, |ui| {
            egui::Panel::left("a")
                .default_size(330.0)
                .show(ui, |ui| app.arbol(ui, &mut acc));
        });
        out.textures_delta.clear();
    }
    assert!(
        acc.iter().any(|a| matches!(a, Accion::Trasladar(o, h, d, Some(antes)) if o == "C01" && h == "SR.COL8" && d == "C01" && antes == "SR.M2")),
        "no hubo traslado"
    );
    assert!(
        !acc.iter().any(|a| matches!(a, Accion::Seleccionar(..))),
        "arrastrar no debe contar como clic"
    );
}

#[test]
fn deshacer_y_rehacer() {
    // Ejemplo: PEM 7.005,15. Tubo 1,10 → 1,25: 7.229,85. Quitar SR.COL8: 6.501,60.
    let mut app = Aplicacion::sin_ventana();
    app.aplicar(vec![Accion::Precio("MT.TUBO16".into(), dec!(1.25))]);
    assert_eq!(app.calc.pem, dec!(7229.85));
    app.aplicar(vec![Accion::QuitarLinea("C01".into(), "SR.COL8".into(), true)]);
    assert_eq!(app.calc.pem, dec!(6501.60));
    app.deshacer();
    assert_eq!(app.calc.pem, dec!(7229.85));
    assert!(app.p.conceptos.contains_key("SR.COL8"), "vuelve con su descompuesto");
    app.deshacer();
    assert_eq!(app.calc.pem, dec!(7005.15));
    app.deshacer(); // nada más que deshacer
    assert_eq!(app.calc.pem, dec!(7005.15));
    app.rehacer();
    app.rehacer();
    assert_eq!(app.calc.pem, dec!(6501.60));
    // Un cambio nuevo borra lo que se podía rehacer
    app.deshacer();
    app.aplicar(vec![Accion::Ci(dec!(10))]);
    assert!(app.historial.rehacer.is_empty());
    // Escribir un resumen letra a letra se deshace de una vez
    let antes = app.p.conceptos["SR.M2"].resumen.clone();
    for t in ["S", "Su", "Sue"] {
        app.aplicar(vec![Accion::Resumen("SR.M2".into(), t.into())]);
    }
    app.deshacer();
    assert_eq!(app.p.conceptos["SR.M2"].resumen, antes);
    // Un error no deja copia en el historial
    let n = app.historial.deshacer.len();
    app.aplicar(vec![Accion::AnadirLinea("C01".into(), "SR.M2".into(), dec!(1))]);
    assert_eq!(app.historial.deshacer.len(), n);
    // Abrir otro presupuesto empieza un historial nuevo
    app.nuevo_presupuesto();
    assert!(app.historial.deshacer.is_empty());
}

#[test]
fn subcontrata_a_ratio_desde_la_interfaz() {
    // SR.M2 entera (salvo %MA) a 38 €/m² → 38,76; SR.COL8 sin tocar 242,75 × 3
    // PEM = 210 × 38,76 + 728,25 = 8.139,60 + 728,25 = 8.867,85
    let mut app = Aplicacion::sin_ventana();
    app.pestana = Pestana::Subcontratar;
    let todos: Vec<String> = app.p.conceptos["SR.M2"]
        .descomposicion
        .iter()
        .map(|l| l.hijo.clone())
        .filter(|h| h != "%MA")
        .collect();
    let paq = ppto_core::ratio::PaqueteRatio {
        codigo: "SUB.SR".into(),
        descripcion: "Suelo radiante".into(),
        unidad: "m2".into(),
        precio: dec!(38),
        medidor: vec!["SR.M2".into()],
        asignaciones: std::collections::BTreeMap::from([("SR.M2".into(), todos)]),
    };
    app.aplicar(vec![Accion::AplicarRatio(paq)]);
    assert_eq!(app.calc.pem, dec!(8867.85));
    app.deshacer();
    assert_eq!(app.calc.pem, dec!(7005.15));
    // La pestaña se pinta con el formulario vacío
    let ctx = Context::default();
    for _ in 0..2 {
        let mut out = ctx.run_ui(RawInput::default(), |ui| {
            let mut acc = Vec::new();
            egui::CentralPanel::default_margins().show(ui, |ui| app.pestana_subcontratar(ui, &mut acc));
        });
        out.textures_delta.clear();
    }
}
#[test]
fn linea_de_tarifa_de_subcontrata() {
    // Tarifa sintética: detector 11,50 €/ud
    let mut app = Aplicacion::sin_ventana();
    app.tarifas.anadir(ppto_tarifas::Tarifa {
        proveedor: "Subco".into(),
        vigencia: "2031".into(),
        especialidad: "PCI".into(),
        notas: String::new(),
        lineas: vec![ppto_tarifas::LineaTarifa {
            referencia: "X010".into(),
            descripcion: "INSTALACION DETECTOR".into(),
            unidad: "ud".into(),
            precio: dec!(11.5),
            revisar_unidad: false,
        }],
    });
    let antes = app.calc.precios["SR.M2"];
    app.aplicar(vec![Accion::Alta(TipoAlta::Linea { padre: "SR.M2".into() })]);
    let e = app.tarifas.buscar("detector")[0];
    if let Some(a) = app.alta.as_mut() {
        a.de_tarifa = true;
        a.buscar = "detector".into();
        a.tarifa_elegida = Some(e);
    }
    pintar(&mut app);
    assert!(
        app.alta.is_some(),
        "la ventana sigue abierta hasta pulsar «Añadir línea»"
    );
    let acc = app.accion_tarifa("SR.M2", e, dec!(0.1)).unwrap();
    assert!(matches!(
        acc,
        Accion::NuevoRecurso {
            naturaleza: Naturaleza::Subcontrata,
            ..
        }
    ));
    app.aplicar(vec![acc]);
    let c = app.p.concepto("SUBCO.X010").unwrap();
    assert_eq!((c.naturaleza, c.precio), (Naturaleza::Subcontrata, dec!(11.5)));
    assert!(app.calc.precios["SR.M2"] > antes);
    // Ya existe: la segunda vez solo añade la línea (en otra partida)
    assert!(matches!(
        app.accion_tarifa("SR.COL8", e, dec!(1)),
        Some(Accion::AnadirLinea(..))
    ));
}
#[test]
fn coeficiente_de_dificultad() {
    let mut app = Aplicacion::sin_ventana();
    let pem = app.calc.pem;
    app.aplicar(vec![Accion::Dificultad {
        ambito: "OBRA".into(),
        coeficiente: dec!(1.2),
        motivo: "obra en altura".into(),
    }]);
    assert!(app.calc.pem > pem);
    let mo = app.p.conceptos["SR.M2"]
        .descomposicion
        .iter()
        .find(|l| l.hijo == "MO.OF1F")
        .unwrap()
        .rendimiento;
    assert_eq!(mo, dec!(0.3), "0,25 × 1,20");
    assert_eq!(app.notas_revision, ["dificultad ×1,20 en OBRA (obra en altura)"]);
    app.deshacer();
    assert_eq!(app.calc.pem, pem);
    assert!(app.notas_revision.is_empty());
    app.rehacer();
    assert_eq!(app.notas_revision.len(), 1);
    // La pestaña Recursos se pinta con el formulario
    let ctx = Context::default();
    for _ in 0..2 {
        let mut out = ctx.run_ui(RawInput::default(), |ui| {
            let mut acc = Vec::new();
            egui::CentralPanel::default_margins().show(ui, |ui| app.pestana_recursos(ui, &mut acc));
        });
        out.textures_delta.clear();
    }
}
#[test]
fn propuestas_de_la_ia() {
    use ppto_core::propuesta::{Confianza, Contenido, EstadoPropuesta as E, Propuesta};
    let mut app = Aplicacion::sin_ventana();
    app.aplicar(vec![Accion::NuevaPartida {
        capitulo: "C01".into(),
        codigo: "P.DIR".into(),
        unidad: "ud".into(),
        resumen: "Partida".into(),
    }]);
    let obra = ppto_propuestas::clave(&app.p);
    for (partida, precio, conf) in [
        ("P.DIR", dec!(57.30), Confianza::Alta),
        ("SR.M2", dec!(5), Confianza::Baja),
    ] {
        app.enlace.bandeja.anadir(
            &obra,
            Propuesta {
                id: 0,
                partida: partida.into(),
                contenido: Contenido::PrecioDirecto(precio),
                confianza: conf,
                justificacion: "prueba".into(),
                autor: "IA".into(),
                estado: E::Pendiente,
                descuentos: Default::default(),
                nueva: None,
            },
        );
    }
    assert_eq!(app.propuestas_pendientes().len(), 2);
    // La ventana se pinta (una de las dos no se puede aplicar: SR.M2 tiene descompuesto)
    app.enlace.ventana = true;
    let ctx = Context::default();
    for _ in 0..2 {
        let mut out = ctx.run_ui(RawInput::default(), |ui| {
            let mut acc = Vec::new();
            let c = ui.ctx().clone();
            app.ventana_propuestas(&c, &mut acc);
            assert!(acc.is_empty());
        });
        out.textures_delta.clear();
    }
    let pem = app.calc.pem;
    app.aplicar(vec![Accion::AceptarPropuesta(1)]);
    assert_eq!(app.calc.precios["P.DIR"], dec!(57.30));
    assert_eq!(app.enlace.bandeja.buscar(1).unwrap().estado, E::Aceptada);
    assert!(app.calc.pem > pem);
    // Deshacer: vuelve a pendiente; rehacer: aceptada
    app.deshacer();
    assert_eq!(app.calc.pem, pem);
    assert_eq!(app.enlace.bandeja.buscar(1).unwrap().estado, E::Pendiente);
    app.rehacer();
    assert_eq!(app.enlace.bandeja.buscar(1).unwrap().estado, E::Aceptada);
    // La imposible da error sin cambiar nada; se puede rechazar
    let p = app.p.clone();
    app.aplicar(vec![Accion::AceptarPropuesta(2)]);
    assert_eq!(app.p, p);
    app.aplicar(vec![Accion::RechazarPropuesta(2)]);
    assert!(app.propuestas_pendientes().is_empty());
}
#[test]
fn descuentos_por_fabricante_desde_recursos() {
    let mut app = Aplicacion::sin_ventana();
    app.p.fijar_tarifa("MT.PANEL", "acme", dec!(20)).unwrap();
    app.recalcular();
    app.aplicar(vec![Accion::Descuento("Acme".into(), dec!(50))]);
    assert_eq!(app.p.concepto("MT.PANEL").unwrap().precio, dec!(10));
    app.aplicar(vec![Accion::Pvp("MT.PANEL".into(), dec!(30))]);
    assert_eq!(app.p.concepto("MT.PANEL").unwrap().precio, dec!(15));
    app.deshacer();
    assert_eq!(app.p.concepto("MT.PANEL").unwrap().precio, dec!(10));
    // La pestaña se pinta con la tabla de descuentos
    let ctx = Context::default();
    for _ in 0..2 {
        let mut out = ctx.run_ui(RawInput::default(), |ui| {
            let mut acc = Vec::new();
            egui::CentralPanel::default_margins().show(ui, |ui| app.pestana_recursos(ui, &mut acc));
        });
        out.textures_delta.clear();
    }
}
/// Coste y venta partida a partida: el desglose de la partida y la
/// tabla de «Resumen y venta» se pintan y la venta sale de PEM + GG + BI.
#[test]
fn coste_y_venta_partida_a_partida() {
    let mut app = Aplicacion::sin_ventana();
    app.gg = dec!(13);
    app.bi = dec!(6);
    let (cap, hijo) = app.sel.clone().unwrap();
    let l = app.calc.lineas[&cap].iter().find(|l| l.hijo == hijo).unwrap().clone();
    let v = venta::venta_partida(l.precio, app.gg, app.bi, 2);
    assert_eq!(v.venta, l.precio + v.gastos_generales + v.beneficio_industrial);
    assert!(v.venta > l.precio);
    let ctx = Context::default();
    for _ in 0..2 {
        let mut out = ctx.run_ui(RawInput::default(), |ui| {
            let mut acc = Vec::new();
            egui::CentralPanel::default_margins().show(ui, |ui| {
                app.pestana_partida(ui, &mut acc);
                app.pestana_resumen(ui, &mut acc);
            });
        });
        out.textures_delta.clear();
    }
    // Margen propio: la partida pasa a PEM × 1,30 y se puede deshacer
    app.aplicar(vec![Accion::Margen(hijo.clone(), Some(dec!(30)))]);
    let m = app.p.concepto(&hijo).unwrap().margen;
    assert_eq!(m, Some(dec!(30)));
    let v = venta::venta_con_margen(l.precio, m, app.gg, app.bi, 2);
    assert_eq!(v.venta, l.precio + ppto_core::redondear(l.precio * dec!(0.30), 2));
    for _ in 0..2 {
        let mut out = ctx.run_ui(RawInput::default(), |ui| {
            let mut acc = Vec::new();
            egui::CentralPanel::default_margins().show(ui, |ui| {
                app.pestana_partida(ui, &mut acc);
                app.pestana_resumen(ui, &mut acc);
            });
        });
        out.textures_delta.clear();
    }
    // Fancoil: coste 599,05 → venta 712,87: sobrecoste 19 %, margen 15,97 %
    let (sc, mv) = sobrecoste_y_margen(dec!(599.05), dec!(712.87));
    assert_eq!(sc.unwrap().round_dp(2), dec!(19.00));
    assert_eq!(mv.unwrap().round_dp(2), dec!(15.97));
    assert_eq!(sobrecoste_y_margen(dec!(0), dec!(0)), (None, None));
    app.deshacer();
    assert_eq!(app.p.concepto(&hijo).unwrap().margen, None);
    // Venta bloqueada: prevalece sobre el margen y se pinta
    app.aplicar(vec![
        Accion::Margen(hijo.clone(), Some(dec!(30))),
        Accion::VentaFija(hijo.clone(), Some(dec!(1))),
    ]);
    let c = app.p.concepto(&hijo).unwrap().clone();
    let b = venta::venta_concepto(l.precio, &c, app.gg, app.bi, 2);
    assert!(b.bloqueada && b.venta == dec!(1));
    for _ in 0..2 {
        let mut out = ctx.run_ui(RawInput::default(), |ui| {
            let mut acc = Vec::new();
            egui::CentralPanel::default_margins().show(ui, |ui| {
                app.pestana_partida(ui, &mut acc);
                app.pestana_resumen(ui, &mut acc);
            });
        });
        out.textures_delta.clear();
    }
    app.aplicar(vec![
        Accion::VentaFija(hijo.clone(), None),
        Accion::Margen(hijo.clone(), None),
    ]);
    assert_eq!(app.p.concepto(&hijo).unwrap().venta_fija, None);
    // Solo partidas
    app.aplicar(vec![Accion::Margen(cap.clone(), Some(dec!(10)))]);
    assert_eq!(app.p.concepto(&cap).unwrap().margen, None);
}

#[test]
fn reaplicar_propuestas_aceptadas_tras_reabrir() {
    use ppto_core::propuesta::{Confianza, Contenido, EstadoPropuesta as E, LineaPropuesta, Propuesta};
    let mut app = Aplicacion::sin_ventana();
    app.aplicar(vec![Accion::NuevaPartida {
        capitulo: "C01".into(),
        codigo: "P.X".into(),
        unidad: "ud".into(),
        resumen: "Partida".into(),
    }]);
    let vacio = app.p.clone();
    let obra = ppto_propuestas::clave(&app.p);
    app.enlace.bandeja.anadir(
        &obra,
        Propuesta {
            id: 0,
            partida: "P.X".into(),
            contenido: Contenido::Descompuesto(vec![LineaPropuesta {
                codigo: "MO.OF1F".into(),
                rendimiento: dec!(2),
                naturaleza: None,
                unidad: String::new(),
                resumen: String::new(),
                precio: None,
                pvp: None,
                fabricante: None,
                grupo: None,
            }]),
            confianza: Confianza::Alta,
            justificacion: "x".into(),
            autor: "IA".into(),
            estado: E::Pendiente,
            descuentos: Default::default(),
            nueva: None,
        },
    );
    app.aplicar(vec![Accion::AceptarPropuesta(1)]);
    assert_eq!(app.calc.precios["P.X"], dec!(49));
    assert!(app.reaplicables().is_empty(), "ya tiene precio");
    // Se vuelve a abrir la obra vacía (como al importar otra vez el ciego)
    app.cargar_presupuesto(vacio);
    assert_eq!(app.reaplicables().len(), 1);
    app.aplicar(vec![Accion::ReaplicarAceptadas]);
    assert_eq!(app.calc.precios["P.X"], dec!(49));
    assert!(app.reaplicables().is_empty());
    app.deshacer();
    assert_eq!(app.reaplicables().len(), 1, "se deshace en un paso");
}

#[test]
fn panel_del_asistente_en_modo_encargo() {
    use ppto_propuestas::conversacion::Autor;
    let mut app = Aplicacion::sin_ventana();
    app.asistente.abierto = true;
    app.asistente.entrada = "Valora la climatización con la oferta".into();
    app.asistente.adjuntos = vec!["C:/ofertas/koolair.pdf".into()];
    let p = app.p.clone();
    app.asistente.enviar(&p).unwrap();
    assert!(app.asistente.entrada.is_empty() && app.asistente.adjuntos.is_empty());
    let ctx = Context::default();
    for _ in 0..2 {
        let mut out = ctx.run_ui(RawInput::default(), |ui| app.panel_asistente(ui));
        out.textures_delta.clear();
    }
    let obra = ppto_propuestas::clave(&app.p);
    let m = app.asistente.mensajes(&obra);
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].autor, Autor::Usuario);
    assert!(!m[0].atendido);
    assert_eq!(m[0].adjuntos, ["C:/ofertas/koolair.pdf"]);
    // El presupuesto no cambia
    assert_eq!(app.p, p);
}

#[test]
fn panel_del_asistente_con_varias_ias() {
    let mut app = Aplicacion::sin_ventana();
    app.asistente.abierto = true;
    let a = app.asistente.con_dos_conexiones();
    assert_eq!(a.conexion, "ChatGPT");
    assert!(a.usar_conexion("Claude"));
    let ctx = Context::default();
    for _ in 0..2 {
        let mut out = ctx.run_ui(RawInput::default(), |ui| app.panel_asistente(ui));
        out.textures_delta.clear();
    }
    let a = app.asistente.con_dos_conexiones();
    assert_eq!(a.conexiones.len(), 2);
}

#[test]
fn ventana_de_biblioteca_guarda_la_obra() {
    let dir = std::env::temp_dir().join(format!("gui-bib-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ruta = dir.join("biblioteca.json");
    // Biblioteca con una tarifa antigua (aviso en naranja)
    ppto_biblioteca::Biblioteca::modificar(&ruta, |b| {
        b.cargar_tarifa("KOOLAIR", "2020", 2020, Default::default());
        b.fijar_descuento("KOOLAIR", Some(dec!(45)));
    })
    .unwrap();
    let mut app = Aplicacion::sin_ventana();
    app.ventana_biblioteca = Some(biblioteca::VentanaBiblioteca::en(Some(ruta.clone())));
    let ctx = Context::default();
    for _ in 0..2 {
        let mut out = ctx.run_ui(RawInput::default(), |ui| {
            let c = ui.ctx().clone();
            app.ventana_biblioteca(&c);
        });
        out.textures_delta.clear();
    }
    assert!(app.ventana_biblioteca.is_some());
    // Guardar la obra (lo mismo que el botón)
    let p = app.p.clone();
    ppto_biblioteca::Biblioteca::modificar(&ruta, |b| {
        b.guardar_obra(&p, "Ejemplo", ppto_biblioteca::Origen::Real, None)
    })
    .unwrap();
    assert_eq!(ppto_biblioteca::Biblioteca::cargar(&ruta).partidas.len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bases_de_precios_copiar_mano_de_obra() {
    // Base sintética (no es una base real): 2 manos de obra, 1 material y una partida
    let dir = std::env::temp_dir().join(format!("gui-bases-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let origen = dir.join("base_sintetica.bc3");
    std::fs::write(
        &origen,
        "~V|PRUEBA|FIEBDC-3/2002|prueba||ANSI|\r\n\
         ~K|\\2\\2\\3\\2\\2\\2\\2\\EUR\\|0|\r\n\
         ~C|BASE##||Base sintetica|0||0|\r\n\
         ~C|CAP#||Capitulo|0||0|\r\n\
         ~C|CRP|UD|COLOCACION REJILLA INTERIOR HASTA 0,5 M2|32.5|010126|1|\r\n\
         ~C|CDC|UD|COLOCACION DIFUSOR CIRCULAR|22.1|010126|1|\r\n\
         ~C|REJ|UD|REJILLA DE PRUEBA|40|010126|3|\r\n\
         ~C|P.REJ|UD|REJILLA COLOCADA|72.5|010126|0|\r\n\
         ~D|BASE##|CAP#\\1\\1\\|\r\n\
         ~D|CAP#|P.REJ\\1\\1\\|\r\n\
         ~D|P.REJ|REJ\\1\\1\\CRP\\1\\1\\|\r\n",
    )
    .unwrap();
    let mut v = bases::VentanaBases::en(Some(dir.join("bases")));
    assert!(v.base.is_none());
    v.anadir(&origen);
    assert!(
        dir.join("bases").join("base_sintetica.bc3").exists(),
        "se guarda una copia"
    );
    assert_eq!(v.base.as_ref().unwrap().filas.len(), 4);
    // Solo mano de obra, buscando «rejilla»
    v.filtro = Some(Naturaleza::ManoObra);
    assert_eq!(v.visibles().len(), 2);
    v.buscar = "rejilla".into();
    let vis: Vec<String> = v.visibles().iter().map(|f| f.codigo.clone()).collect();
    assert_eq!(vis, ["CRP"]);
    // La ventana se dibuja
    let mut app = Aplicacion::sin_ventana();
    let origen_p = std::sync::Arc::clone(&v.base.as_ref().unwrap().p);
    app.ventana_bases = Some(v);
    let ctx = Context::default();
    for _ in 0..2 {
        let mut acc = Vec::new();
        let mut out = ctx.run_ui(RawInput::default(), |ui| {
            let c = ui.ctx().clone();
            app.ventana_bases(&c, &mut acc);
        });
        out.textures_delta.clear();
    }
    assert!(app.ventana_bases.is_some());
    // Copiar y añadir CRP a SR.M2: entra al final, detrás del 2 % de medios
    // auxiliares (que solo afecta a lo anterior), 29,89 + 32,50 = 62,39 €/m2
    app.aplicar(vec![Accion::CopiarDeBase {
        origen: std::sync::Arc::clone(&origen_p),
        nombre: "base_sintetica.bc3".into(),
        codigos: vec!["CRP".into()],
        partida: Some("SR.M2".into()),
    }]);
    assert_eq!(app.p.precio("CRP").unwrap(), dec!(32.50));
    assert!(app.p.conceptos["SR.M2"].descomposicion.iter().any(|l| l.hijo == "CRP"));
    assert_eq!(app.p.precio("SR.M2").unwrap(), dec!(62.39));
    // Deshacer lo quita todo
    app.deshacer();
    assert!(!app.p.conceptos.contains_key("CRP"));
    // Copiar una partida sin colocarla: trae su descompuesto y el PEM no cambia
    let pem = app.p.pem().unwrap();
    app.aplicar(vec![Accion::CopiarDeBase {
        origen: origen_p,
        nombre: "base_sintetica.bc3".into(),
        codigos: vec!["P.REJ".into()],
        partida: None,
    }]);
    assert_eq!(app.p.precio("P.REJ").unwrap(), dec!(72.50));
    assert_eq!(app.p.pem().unwrap(), pem);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn la_barra_de_menus_usa_simbolos_que_existen_en_la_fuente() {
    // Las flechas de deshacer/rehacer deben estar en la fuente de egui
    // (↩ y ↪ no están y saldrían como un cuadro).
    let ctx = Context::default();
    let mut out = ctx.run_ui(RawInput::default(), |_| {});
    out.textures_delta.clear();
    for c in ['⟲', '⟳', '⏵', '⏷', '⏶', '🖹', '≠'] {
        assert!(
            ctx.fonts_mut(|f| f.has_glyph(&egui::FontId::default(), c)),
            "falta el símbolo {c}"
        );
    }
}

#[test]
fn resumen_de_un_recurso_creado_sin_texto() {
    // Recurso creado sin resumen ni precio (como R0001): se le pone texto
    // y unidad después, con deshacer.
    let mut app = Aplicacion::sin_ventana();
    app.aplicar(vec![Accion::NuevoRecurso {
        codigo: "R0001".into(),
        unidad: String::new(),
        resumen: String::new(),
        naturaleza: Naturaleza::ManoObra,
        precio: dec!(0),
        en: "SR.M2".into(),
        cantidad: dec!(1),
    }]);
    app.sel = Some(("C01".into(), "SR.M2".into()));
    // La tabla del descompuesto y la de recursos se dibujan con campos editables
    let ctx = Context::default();
    for pestana in [Pestana::Partida, Pestana::Recursos] {
        app.pestana = pestana;
        let mut acc = Vec::new();
        let mut out = ctx.run_ui(RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                if app.pestana == Pestana::Partida {
                    app.pestana_partida(ui, &mut acc);
                } else {
                    app.pestana_recursos(ui, &mut acc);
                }
            });
        });
        out.textures_delta.clear();
    }
    app.aplicar(vec![
        Accion::Resumen("R0001".into(), "Oficial 1ª calefactor".into()),
        Accion::Unidad("R0001".into(), "h".into()),
    ]);
    assert_eq!(app.p.conceptos["R0001"].resumen, "Oficial 1ª calefactor");
    assert_eq!(app.p.conceptos["R0001"].unidad, "h");
}

#[test]
fn vistas_presupuesto_y_precios() {
    let mut app = Aplicacion::sin_ventana();
    // Precios y márgenes: SR.M2 cuesta 29,89 y se vende a 35,57 (GG 13 % + BI 6 %);
    // margen sobre venta (35,57 − 29,89) / 35,57 = 15,97 %
    let f = app.filas_precios();
    assert_eq!(f.len(), 2);
    let m2 = f.iter().find(|x| x.codigo == "SR.M2").unwrap();
    assert_eq!(
        (m2.coste, m2.venta, m2.margen),
        (dec!(29.89), dec!(35.57), Some(dec!(15.97)))
    );
    // 210 m² × 35,57 = 7.469,70
    assert_eq!(m2.importe_venta, dec!(7469.70));
    // Ordenar por importe (de mayor a menor) y filtrar por texto
    app.filtro_precios.orden = vistas::Orden::Importe;
    app.filtro_precios.descendente = true;
    assert_eq!(app.filas_precios_filtradas()[0].codigo, "SR.M2");
    app.filtro_precios.texto = "colector".into();
    let v = app.filas_precios_filtradas();
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].codigo, "SR.COL8");
    app.filtro_precios.texto.clear();
    // Margen por debajo del 10 %: ninguna (las dos rondan el 16 %)
    app.filtro_precios.margen_bajo = Some(dec!(10));
    assert!(app.filas_precios_filtradas().is_empty());
    app.filtro_precios.margen_bajo = None;
    // Sobrecoste propio del 30 % en SR.COL8 desde la vista: 242,75 × 1,30 = 315,58
    app.aplicar(vec![Accion::Margen("SR.COL8".into(), Some(dec!(30)))]);
    let c = app.filas_precios().into_iter().find(|x| x.codigo == "SR.COL8").unwrap();
    assert_eq!(c.venta, dec!(315.58));
    // Las dos vistas se dibujan; plegar un capítulo en «Presupuesto»
    app.plegados.insert("C01".into());
    let ctx = Context::default();
    for p in [Pestana::Presupuesto, Pestana::Precios] {
        app.pestana = p;
        let mut acc = Vec::new();
        let mut out = ctx.run_ui(RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                if p == Pestana::Presupuesto {
                    app.pestana_presupuesto(ui, &mut acc);
                } else {
                    app.pestana_precios(ui, &mut acc);
                }
            });
        });
        out.textures_delta.clear();
    }
}

#[test]
fn vista_de_mediciones() {
    let mut app = Aplicacion::sin_ventana();
    let ctx = Context::default();
    app.pestana = Pestana::Mediciones;
    for filtro in ["", "planta primera"] {
        app.filtro_mediciones = filtro.into();
        let mut acc = Vec::new();
        let mut out = ctx.run_ui(RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| app.pestana_mediciones(ui, &mut acc));
        });
        out.textures_delta.clear();
        assert!(acc.is_empty(), "dibujar no cambia nada");
    }
    // Lo que hace la celda «Uds» al cambiar Planta baja de 2 a 4 colectores:
    // 5 ud × 242,75 = 1.213,75 (antes 728,25) → PEM 7.005,15 + 485,50 = 7.490,65
    let mut m = app.p.mediciones[&("C01".to_owned(), "SR.COL8".to_owned())].clone();
    m.lineas[0].unidades = Some(dec!(4));
    app.aplicar(vec![Accion::Medicion("C01".into(), "SR.COL8".into(), m)]);
    assert_eq!(app.p.pem().unwrap(), dec!(7490.65));
}

#[test]
fn recursos_horas_por_oficio_y_por_fabricante() {
    let mut app = Aplicacion::sin_ventana();
    // Oficial: 210 m² × 0,25 h + 3 ud × 1,5 h = 57 h; ayudante: 52,5 + 3 = 55,5 h
    let h = app.horas_por_oficio();
    let of = h.iter().find(|x| x.0 == "MO.OF1F").unwrap();
    let ay = h.iter().find(|x| x.0 == "MO.AYUF").unwrap();
    assert_eq!((of.2, ay.2), (dec!(57.00), dec!(55.50)));
    // Colector de tarifa: PVP 300 con 40 % → neto 180; 3 ud → 900 a PVP, 540 neto
    app.p.conceptos.get_mut("MT.COL8").unwrap().tarifa = Some(ppto_core::PrecioTarifa {
        fabricante: "COLEC".into(),
        grupo: None,
        pvp: dec!(300),
    });
    app.p.fijar_descuento("COLEC", dec!(40)).unwrap();
    app.recalcular();
    let f = app.por_fabricante();
    assert_eq!(f.len(), 1);
    assert_eq!((f[0].referencias, f[0].pvp, f[0].neto), (1, dec!(900.00), dec!(540.00)));
    // La vista se dibuja con los dos resúmenes
    let ctx = Context::default();
    let mut acc = Vec::new();
    let mut out = ctx.run_ui(RawInput::default(), |ui| {
        egui::CentralPanel::default().show(ui, |ui| app.pestana_recursos(ui, &mut acc));
    });
    out.textures_delta.clear();
}

#[test]
fn comparar_y_plantillas_en_la_interfaz() {
    // Comparar la obra (con 4 colectores) con la versión original: +242,75 €
    let mut app = Aplicacion::sin_ventana();
    let original = app.p.clone();
    app.aplicar(vec![Accion::Rendimiento("C01".into(), "SR.COL8".into(), dec!(4))]);
    app.abrir_comparacion(&original, "revisión R1".into());
    let v = app.ventana_comparar.as_ref().unwrap();
    assert_eq!(v.resultado.diferencia(), dec!(242.75));
    assert_eq!(v.resultado.cambios.len(), 1);
    let ctx = Context::default();
    for _ in 0..2 {
        let mut acc = Vec::new();
        let mut out = ctx.run_ui(RawInput::default(), |ui| {
            let c = ui.ctx().clone();
            app.ventana_comparar(&c, &mut acc);
        });
        out.textures_delta.clear();
    }
    assert!(app.ventana_comparar.is_some());
    // Nuevo desde plantilla: PCI con observaciones, legalización y contradictorios, sin precios
    app.nuevo_desde_plantilla(ppto_core::plantillas::Plantilla::Pci);
    assert!(app.p.conceptos.contains_key("C00") && app.p.conceptos.contains_key("C99"));
    assert!(app.p.conceptos.contains_key("P01.02"), "detector óptico");
    assert_eq!(app.calc.pem, dec!(0));
    assert!(!app.cambios || app.fichero.is_none());
}

#[test]
fn ctrl_c_en_una_ventana_y_ctrl_v_en_otra() {
    use eframe::egui::Event;
    let pulsar = |app: &mut Aplicacion, ctx: &Context, ev: Event| {
        let mut acc = Vec::new();
        let mut out = ctx.run_ui(
            RawInput {
                events: vec![ev],
                ..Default::default()
            },
            |ui| app.copiar_pegar_teclado(&ui.ctx().clone(), &mut acc),
        );
        out.textures_delta.clear();
        app.aplicar(acc);
    };
    // Ventana A (ejemplo): SR.M2 elegida, Ctrl+C
    let mut a = Aplicacion::sin_ventana();
    a.sel = Some(("C01".into(), "SR.M2".into()));
    pulsar(&mut a, &Context::default(), Event::Copy);
    let texto = a.copiar_al_sistema.take().expect("va al portapapeles del sistema");
    // Ventana B (obra vacía con un capítulo elegido): Ctrl+V → 210 m² × 29,89 = 6.276,90
    let mut b = Aplicacion::sin_ventana();
    b.nuevo_presupuesto();
    b.p.nuevo_capitulo("OBRA", "X1", "Calefacción").unwrap();
    b.recalcular();
    b.aplicar(vec![Accion::CapituloActivo("X1".into())]);
    pulsar(&mut b, &Context::default(), Event::Paste(texto));
    assert_eq!(b.calc.pem, dec!(6276.90));
    assert_eq!(b.sel, Some(("X1".into(), "SR.M2".into())));
    // Se puede deshacer
    b.deshacer();
    assert_eq!(b.calc.pem, dec!(0));
    // Texto cualquiera: no pega nada
    pulsar(&mut b, &Context::default(), Event::Paste("hola".into()));
    assert_eq!(b.calc.pem, dec!(0));
}

#[test]
fn la_barra_de_menus_se_dibuja() {
    let mut app = Aplicacion::sin_ventana();
    let ctx = Context::default();
    for _ in 0..2 {
        let mut acc = Vec::new();
        let mut out = ctx.run_ui(RawInput::default(), |ui| {
            egui::Panel::top("barra").show(ui, |ui| app.barra_superior(ui, &mut acc));
        });
        out.textures_delta.clear();
    }
}

#[test]
fn precio_vinculado_a_oferta_y_documento_en_cualquier_carpeta() {
    // Oferta sintética; el documento está en una carpeta cualquiera
    let dir = std::env::temp_dir().join(format!("gui-origen-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Obra X").join("ofertas")).unwrap();
    let pdf = dir.join("Obra X").join("ofertas").join("of-77.pdf");
    std::fs::write(&pdf, b"%PDF-1.4 sintetico").unwrap();
    let mut bib = ppto_biblioteca::Biblioteca::default();
    bib.anadir_oferta(
        "ACME",
        ppto_biblioteca::Oferta {
            numero: "OF-77".into(),
            fecha: "2026-10-01".into(),
            validez: Some("2026-10-15".into()),
            obra: String::new(),
            lineas: vec![ppto_biblioteca::LineaOferta {
                referencia: "T16".into(),
                descripcion: "Tubo 16".into(),
                unidad: "m".into(),
                neto: dec!(0.95),
                cantidad: None,
            }],
            documento: Some(pdf.display().to_string()),
        },
    );
    let mut app = Aplicacion::sin_ventana();
    let precio = app.p.concepto("MT.TUBO16").unwrap().precio;
    let mut f = ofertas::FormOrigen::con_biblioteca("MT.TUBO16", "Tubo", precio, None, bib);
    f.desde_biblioteca("ACME", "OF-77", Some("T16"));
    let o = f.origen().unwrap();
    assert_eq!(o.neto, Some(dec!(0.95)));
    assert_eq!(o.documento.as_deref(), Some(pdf.display().to_string().as_str()));
    app.aplicar(vec![Accion::Origen("MT.TUBO16".into(), Some(Box::new(o)), true)]);
    assert_eq!(app.p.concepto("MT.TUBO16").unwrap().precio, dec!(0.95));
    // Otro recurso con un documento que ya no está (se marca para volver a vincular)
    let mut perdido = app.p.concepto("MT.TUBO16").unwrap().origen.clone().unwrap();
    perdido.documento = Some(dir.join("movido.pdf").display().to_string());
    app.aplicar(vec![Accion::Origen("MT.PANEL".into(), Some(Box::new(perdido)), false)]);
    assert!(app.docs.existe(&pdf.display().to_string(), 0.0));
    assert!(!app.docs.existe(&dir.join("movido.pdf").display().to_string(), 0.0));
    // Se pintan la tabla de recursos y el formulario
    app.abrir_origen("MT.PANEL");
    let ctx = Context::default();
    for _ in 0..2 {
        let mut out = ctx.run_ui(RawInput::default(), |ui| {
            let mut acc = Vec::new();
            app.pestana_recursos(ui, &mut acc);
            let c = ui.ctx().clone();
            app.ventana_origen(&c, &mut acc);
            assert!(acc.is_empty());
        });
        out.textures_delta.clear();
    }
    assert!(app.form_origen.is_some());
    // Deshacer devuelve el precio anterior
    app.aplicar(vec![Accion::Origen("MT.PANEL".into(), None, false)]);
    assert!(app.p.concepto("MT.PANEL").unwrap().origen.is_none());
    app.deshacer();
    assert!(app.p.concepto("MT.PANEL").unwrap().origen.is_some());
    app.deshacer();
    app.deshacer();
    assert_eq!(app.p.concepto("MT.TUBO16").unwrap().precio, precio);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn aviso_de_version_nueva() {
    let mut app = Aplicacion::sin_ventana();
    app.actualizacion.estado = actualizar::Estado::Disponible(ppto_actualizar::Version {
        numero: "9.9.9".into(),
        notas: "Novedades".into(),
        pagina: "https://example.invalid".into(),
        zip: Some("https://example.invalid/x.zip".into()),
        tamano: 70 * 1024 * 1024,
    });
    app.actualizacion.ventana = true;
    let ctx = Context::default();
    for _ in 0..2 {
        let mut out = ctx.run_ui(RawInput::default(), |ui| {
            let c = ui.ctx().clone();
            app.boton_actualizar(ui);
            app.ventana_actualizar(&c);
        });
        out.textures_delta.clear();
    }
    assert!(app.actualizacion.ventana);
    assert!(matches!(app.actualizacion.estado, actualizar::Estado::Disponible(_)));
}

#[test]
fn importar_modelo_bim() {
    let mut app = Aplicacion::sin_ventana();
    let ruta = std::path::PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../ppto-modelo/tests/datos/instalacion-sintetica.ifc"
    ));
    app.abrir_modelo(ruta);
    let ctx = Context::default();
    for _ in 0..200 {
        let mut out = ctx.run_ui(RawInput::default(), |ui| {
            let c = ui.ctx().clone();
            app.ventana_modelo(&c);
        });
        out.textures_delta.clear();
        if app.ventana_modelo.as_ref().is_some_and(|v| v.modelo.is_some()) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let m = match &app.ventana_modelo.as_ref().unwrap().modelo {
        Some(Ok(m)) => m.clone(),
        otro => panic!("{otro:?}"),
    };
    assert_eq!(m.elementos.len(), 9);
    // Añadir a la obra de ejemplo: se puede deshacer
    let pem = app.p.pem().unwrap();
    let antes = app.p.concepto(&app.p.raiz).unwrap().descomposicion.len();
    let pm = ppto_modelo::a_presupuesto(&m, app.ventana_modelo.as_ref().unwrap().opciones).unwrap();
    let hijos: Vec<String> = pm
        .concepto("OBRA")
        .unwrap()
        .descomposicion
        .iter()
        .map(|l| l.hijo.clone())
        .collect();
    let f = ppto_core::fragmento::copiar(&pm, "OBRA", &hijos).unwrap();
    let raiz = app.p.raiz.clone();
    app.aplicar(vec![Accion::PegarFragmento(Box::new(f), raiz)]);
    assert_eq!(app.p.concepto(&app.p.raiz).unwrap().descomposicion.len(), antes + 3);
    assert_eq!(app.p.pem().unwrap(), pem, "sin precios");
    app.deshacer();
    assert_eq!(app.p.concepto(&app.p.raiz).unwrap().descomposicion.len(), antes);
}

#[test]
fn contradictorios_y_certificaciones() {
    use obra::{OpObra, PestanaObra};
    let mut app = Aplicacion::sin_ventana();
    let ctx = Context::default();
    let pinta = |app: &mut Aplicacion| {
        for _ in 0..2 {
            let mut out = ctx.run_ui(RawInput::default(), |ui| {
                let c = ui.ctx().clone();
                app.ventana_obra(&c);
            });
            out.textures_delta.clear();
        }
    };
    app.abrir_obra(PestanaObra::Contradictorios);
    pinta(&mut app);
    app.aplicar(vec![Accion::Obra(OpObra::NuevoContradictorio {
        resumen: "Extra".into(),
        fecha: "01/11/2026".into(),
        nota: "DF".into(),
    })]);
    assert!(app.p.seguimiento.contradictorios.contains_key("CTR-01"));
    app.aplicar(vec![Accion::AnadirLinea("CTR-01".into(), "SR.M2".into(), dec!(10))]);
    app.aplicar(vec![Accion::Obra(OpObra::Estado(
        "CTR-01".into(),
        ppto_core::seguimiento::EstadoContradictorio::Aprobado,
    ))]);
    pinta(&mut app);
    let r = app.p.resumen_contrato().unwrap();
    assert_eq!(r.contratado, dec!(7005.15));
    assert!(r.aprobados > Decimal::ZERO);
    // PDF del contradictorio con la firma
    let c = app.p.seguimiento.contradictorios["CTR-01"].clone();
    assert!(app.pdf_contradictorio("CTR-01", &c).unwrap().starts_with(b"%PDF"));
    // Certificación: del mes, 100 %, cerrar; deshacer
    app.abrir_obra(PestanaObra::Certificaciones);
    pinta(&mut app);
    app.aplicar(vec![Accion::Obra(OpObra::NuevaCertificacion("30/11/2026".into()))]);
    app.aplicar(vec![
        Accion::Obra(OpObra::Mes(1, "C01".into(), "SR.M2".into(), dec!(50))),
        Accion::Obra(OpObra::Completa(1, "C01".into(), "SR.COL8".into())),
    ]);
    pinta(&mut app);
    assert!(app.ventana_obra.as_ref().unwrap().numero == Some(1));
    app.aplicar(vec![Accion::Obra(OpObra::Cerrar(1, app.gg, app.bi))]);
    assert!(app.p.seguimiento.certificaciones[0].cerrada);
    // Error del motor: no queda en el historial
    let antes = app.historial.deshacer.len();
    app.aplicar(vec![Accion::Obra(OpObra::Mes(
        1,
        "C01".into(),
        "SR.M2".into(),
        dec!(1),
    ))]);
    assert_eq!(app.historial.deshacer.len(), antes);
    app.deshacer();
    assert!(!app.p.seguimiento.certificaciones[0].cerrada);
    pinta(&mut app);
    // Se guarda con la obra (y la retención con sus datos)
    app.retencion = dec!(5);
    let d = app.datos_obra();
    let mut otra = Aplicacion::sin_ventana();
    otra.aplicar_datos_obra(&d);
    assert_eq!(otra.retencion, dec!(5));
}

#[test]
fn informes_de_pedidos_mano_de_obra_y_coste() {
    let mut app = Aplicacion::sin_ventana();
    app.p.fijar_descuento("SCHAKO", dec!(40)).unwrap();
    app.p.fijar_tarifa("MT.PANEL", "schako", dec!(20)).unwrap();
    app.recalcular();
    let ctx = Context::default();
    for t in informes::TipoInforme::TODOS {
        app.abrir_informe(t);
        for _ in 0..2 {
            let mut out = ctx.run_ui(RawInput::default(), |ui| {
                let c = ui.ctx().clone();
                app.ventana_informes(&c);
            });
            out.textures_delta.clear();
        }
        assert_eq!(app.ventana_informes.as_ref().unwrap().tipo, t);
    }
}

#[test]
fn arbol_en_rojo_y_naranja_lo_que_falta() {
    use ppto_core::valoracion::Estado;
    let mut app = Aplicacion::sin_ventana();
    let raiz = app.p.raiz.clone();
    app.p.nuevo_capitulo(&raiz, "C9", "Pruebas").unwrap();
    app.p.nueva_partida("C9", "P.VACIA", "ud", "Sin valorar").unwrap();
    app.recalcular();
    assert_eq!(app.calc.estados["P.VACIA"], Estado::SinPrecio);
    assert_eq!(app.calc.estados["C9"], Estado::SinPrecio);
    assert_eq!(app.calc.estados["C01"], Estado::Valorada);
    let ctx = Context::default();
    for _ in 0..2 {
        let mut out = ctx.run_ui(RawInput::default(), |ui| {
            let mut acc = Vec::new();
            app.arbol(ui, &mut acc);
        });
        out.textures_delta.clear();
    }
    assert!(ctx.fonts_mut(|f| f.has_glyph(&egui::FontId::default(), '■')));
}

#[test]
fn f5_abre_renglon_encima() {
    let mut app = Aplicacion::sin_ventana();
    let orden = |app: &Aplicacion, c: &str| -> Vec<String> {
        app.p.conceptos[c]
            .descomposicion
            .iter()
            .map(|l| l.hijo.clone())
            .collect()
    };
    let antes = orden(&app, "C01");
    let segunda = antes[1].clone();
    // F5 sin nada bajo el ratón: encima de la partida elegida
    app.sel = Some(("C01".into(), segunda.clone()));
    let ctx = Context::default();
    let mut entrada = RawInput::default();
    entrada.events.push(egui::Event::Key {
        key: egui::Key::F5,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    let mut acc = Vec::new();
    let mut out = ctx.run_ui(entrada, |ui| app.tecla_f5(ui.ctx(), &mut acc));
    out.textures_delta.clear();
    assert_eq!(acc.len(), 1);
    app.aplicar(acc);
    let a = app.alta.as_ref().unwrap();
    assert_eq!(a.antes.as_deref(), Some(segunda.as_str()));
    // Lo que haría «Crear partida» en la ventana de alta
    app.aplicar(vec![
        Accion::NuevaPartida {
            capitulo: "C01".into(),
            codigo: "P.NUEVA".into(),
            unidad: "ud".into(),
            resumen: "Nueva".into(),
        },
        Accion::Colocar("C01".into(), "P.NUEVA".into(), segunda.clone()),
    ]);
    let ahora = orden(&app, "C01");
    assert_eq!(ahora[1], "P.NUEVA");
    assert_eq!(ahora[2], segunda);
    // Un solo «Deshacer» la quita
    app.deshacer();
    assert_eq!(orden(&app, "C01"), antes);
    // Línea nueva encima en un descompuesto
    let d = orden(&app, "SR.M2");
    app.aplicar(vec![Accion::Insertar(
        TipoAlta::Linea { padre: "SR.M2".into() },
        d[0].clone(),
    )]);
    assert_eq!(app.alta.as_ref().unwrap().antes.as_deref(), Some(d[0].as_str()));
}

#[test]
fn gg_bi_iva_y_ciego_se_guardan_con_la_obra() {
    let dir = std::env::temp_dir().join(format!("gui-datos-obra-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ruta = dir.join("obra.sqlite");
    let mut app = Aplicacion::sin_ventana();
    app.gg = dec!(15);
    app.bi = dec!(7.5);
    app.iva = dec!(10);
    app.fichero = Some(ruta.clone());
    app.guardar(false);
    assert!(!app.cambios);
    // Datos de la obra guardados con la revisión
    let a = Almacen::abrir(&ruta).unwrap();
    let rev = a.listar_revisiones().unwrap().last().unwrap().id;
    let d = a.datos_obra(rev).unwrap();
    assert_eq!(d["gg"], "15");
    assert!(!d.contains_key("ciego"));
    drop(a);
    // Al abrir en otra sesión vuelven GG, BI e IVA (no los de por defecto)
    let mut otra = Aplicacion::sin_ventana();
    otra.abrir(ruta.clone());
    assert_eq!((otra.gg, otra.bi, otra.iva), (dec!(15), dec!(7.5), dec!(10)));
    // Un ciego que ya no está en su sitio no impide abrir
    let mut d = d.clone();
    d.insert("ciego".into(), dir.join("no-existe.xlsx").display().to_string());
    otra.aplicar_datos_obra(&d);
    assert!(otra.ciego.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn se_pintan_las_ventanas_sin_pruebas() {
    // Legalización, tarifas de subcontratas, Subcontratar (ratio) y
    // Recursos (dificultad y descuentos): que se abran y se pinten sin
    // romperse con el ejemplo.
    let mut app = Aplicacion::sin_ventana();
    app.ventana_legalizacion = Some(legalizacion::VentanaLegalizacion::abrir(&app.p));
    app.abrir_tarifas();
    let ctx = Context::default();
    for _ in 0..3 {
        let mut out = ctx.run_ui(RawInput::default(), |ui| {
            let c = ui.ctx().clone();
            app.ventana_legalizacion(&c);
            app.ventana_tarifas(&c);
            let mut acc = Vec::new();
            app.pestana_subcontratar(ui, &mut acc);
            app.pestana_recursos(ui, &mut acc);
            app.pestana_resumen(ui, &mut acc);
            assert!(acc.is_empty());
        });
        out.textures_delta.clear();
    }
    assert!(app.ventana_legalizacion.is_some());
    assert!(app.ventana_tarifas.is_some());
}

#[test]
fn autoguardado_y_recuperacion() {
    let dir = std::env::temp_dir().join(format!("gui-autoguardado-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let propia = dir.join(format!("obra-{}.json", std::process::id()));
    let mut app = Aplicacion::sin_ventana();
    app.recuperacion = recuperacion::Recuperacion::en(&dir, 0);
    app.gg = dec!(16);
    app.aplicar(vec![Accion::Precio("MT.TUBO16".into(), dec!(1.30))]);
    assert!(app.cambios);
    let pem = app.calc.pem;
    let ctx = Context::default();
    let fotograma = |app: &mut Aplicacion, t: f64| {
        let entrada = RawInput {
            time: Some(t),
            ..RawInput::default()
        };
        let mut out = ctx.run_ui(entrada, |ui| app.autoguardar(ui.ctx()));
        out.textures_delta.clear();
    };
    // Cerrar la ventana a los pocos segundos de un cambio: copia en el acto
    let mut cierre = RawInput {
        time: Some(0.5),
        ..RawInput::default()
    };
    cierre
        .viewports
        .entry(egui::ViewportId::ROOT)
        .or_default()
        .events
        .push(egui::ViewportEvent::Close);
    let mut out = ctx.run_ui(cierre, |ui| app.autoguardar(ui.ctx()));
    out.textures_delta.clear();
    assert!(propia.exists(), "copia al cerrar");
    std::fs::remove_file(&propia).unwrap();
    app.recuperacion = recuperacion::Recuperacion::en(&dir, 0);
    fotograma(&mut app, 1.0);
    assert!(!propia.exists(), "la primera copia, al cabo de un minuto");
    fotograma(&mut app, 30.0);
    assert!(!propia.exists());
    fotograma(&mut app, 62.0);
    assert!(propia.exists(), "copia de recuperación escrita");
    // «Se cierra de golpe»: otra sesión la encuentra y la recupera
    std::fs::rename(&propia, dir.join("obra-1.json")).unwrap();
    let mut otra = Aplicacion::sin_ventana();
    otra.recuperacion = recuperacion::Recuperacion::en(&dir, 0);
    assert_eq!(otra.recuperacion.pendientes.len(), 1);
    let (_, c) = otra.recuperacion.pendientes.remove(0);
    otra.recuperar(c);
    assert!(otra.cambios);
    assert_eq!(otra.calc.pem, pem);
    assert_eq!(otra.gg, dec!(16));
    // Al guardar ya no hay cambios y la copia se borra
    app.fichero = Some(dir.join("obra.sqlite"));
    app.guardar(false);
    std::fs::write(&propia, "x").unwrap();
    fotograma(&mut app, 130.0);
    assert!(!propia.exists(), "sin cambios, la copia sobra");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn avisa_si_otro_tiene_la_obra_abierta() {
    let dir = std::env::temp_dir().join(format!("gui-abierta-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ruta = dir.join("obra.sqlite");
    let mut app = Aplicacion::sin_ventana();
    app.fichero = Some(ruta.clone());
    app.guardar(false);
    // Al pintar se deja el aviso propio
    let ctx = Context::default();
    let pinta = |app: &mut Aplicacion| {
        let mut out = ctx.run_ui(RawInput::default(), |ui| {
            let c = ui.ctx().clone();
            app.aviso_obra_abierta(&c);
            app.ventana_confirmar_abrir(&c);
        });
        out.textures_delta.clear();
    };
    pinta(&mut app);
    let aviso = ppto_db::bloqueo::ruta_aviso(&ruta);
    assert!(aviso.is_file());
    // Otra persona: se pregunta antes de abrir
    let ahora = ppto_db::bloqueo::ahora();
    std::fs::write(
        &aviso,
        format!(r#"{{"usuario":"ana","equipo":"PC-OFICINA","pid":1,"desde":{ahora},"latido":{ahora}}}"#),
    )
    .unwrap();
    let mut otra = Aplicacion::sin_ventana();
    otra.abrir(ruta.clone());
    assert!(otra.fichero.is_none(), "no se abre sin preguntar");
    assert_eq!(otra.confirmar_abrir.as_ref().unwrap().1.usuario, "ana");
    pinta(&mut otra);
    otra.confirmar_abrir = None;
    otra.abrir_sin_preguntar(ruta.clone());
    assert_eq!(otra.fichero.as_ref(), Some(&ruta));
    // Al cerrar la obra (ejemplo nuevo) se quita el aviso propio
    pinta(&mut otra);
    otra.cargar_ejemplo();
    pinta(&mut otra);
    assert!(!aviso.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn se_pinta_toda_la_ventana_en_cada_vista() {
    // Ejemplo y presupuestos aleatorios grandes, en todas las vistas, con
    // una partida, un capítulo o nada seleccionado: sin romperse.
    let ctx = Context::default();
    let mut obras = vec![suelo_radiante()];
    obras.extend((0..6).map(|s| ppto_core::ejemplos::aleatorio(s, true)));
    for p in obras {
        let mut app = Aplicacion::sin_ventana();
        app.cargar_presupuesto(p);
        let primera = app.primera_partida(&app.p.raiz.clone());
        let capitulo = app
            .p
            .concepto(&app.p.raiz)
            .unwrap()
            .descomposicion
            .first()
            .map(|l| (app.p.raiz.clone(), l.hijo.clone()));
        for sel in [primera, capitulo, None] {
            for (vista, _, _) in vistas::VISTAS {
                app.sel = sel.clone();
                app.pestana = vista;
                for _ in 0..2 {
                    let mut out = ctx.run_ui(RawInput::default(), |ui| app.pantalla(ui));
                    out.textures_delta.clear();
                }
            }
        }
    }
}

#[test]
fn registrar_pedido_y_control_de_compras() {
    use obra::{OpObra, PestanaObra};
    let mut app = Aplicacion::sin_ventana();
    let ctx = Context::default();
    // Desde Informes: «Registrar en la obra»
    app.abrir_informe(informes::TipoInforme::PedidoFabricante);
    let x = ppto_core::pedidos::pedidos(&app.p)
        .unwrap()
        .into_iter()
        .find(|x| x.tipo == ppto_core::pedidos::TipoPedido::Material)
        .unwrap();
    app.registrar
        .borrow_mut()
        .replace((x.clone(), "01/11/2026".into(), "OBRA".into()));
    let mut out = ctx.run_ui(RawInput::default(), |ui| {
        let c = ui.ctx().clone();
        app.ventana_informes(&c);
    });
    out.textures_delta.clear();
    assert_eq!(app.p.seguimiento.pedidos.len(), 1);
    let codigo = app.p.seguimiento.pedidos[0].lineas[0].codigo.clone();
    // Obra → Pedidos y Control de compras
    for t in [PestanaObra::Pedidos, PestanaObra::Compras] {
        app.abrir_obra(t);
        for _ in 0..2 {
            let mut out = ctx.run_ui(RawInput::default(), |ui| {
                let c = ui.ctx().clone();
                app.ventana_obra(&c);
            });
            out.textures_delta.clear();
        }
    }
    app.aplicar(vec![
        Accion::Obra(OpObra::EstadoPedido(1, ppto_core::compras::EstadoPedido::Enviado)),
        Accion::Obra(OpObra::Recibido(1, codigo.clone(), dec!(1))),
    ]);
    assert_eq!(app.p.seguimiento.pedidos[0].situacion(), "Recibido en parte");
    let c = app.p.control_compras().unwrap();
    assert_eq!(c.iter().find(|r| r.codigo == codigo).unwrap().recibido, dec!(1));
    // Se deshace
    app.deshacer();
    assert_eq!(app.p.seguimiento.pedidos[0].situacion(), "Enviado");
    app.abrir_obra(PestanaObra::Pedidos);
    let mut out = ctx.run_ui(RawInput::default(), |ui| {
        let c = ui.ctx().clone();
        app.ventana_obra(&c);
    });
    out.textures_delta.clear();
}

#[test]
fn coste_real_desde_la_obra() {
    use obra::{OpObra, PestanaObra};
    let mut app = Aplicacion::sin_ventana();
    let ctx = Context::default();
    let g = ppto_core::coste_real::Gasto {
        numero: 0,
        fecha: "01/11/2026".into(),
        tipo: ppto_core::coste_real::TipoGasto::Material,
        proveedor: "Almacén".into(),
        documento: "F-12".into(),
        concepto: "Tubo".into(),
        capitulo: Some("C01".into()),
        horas: None,
        importe: dec!(500),
        pedido: None,
    };
    app.aplicar(vec![Accion::Obra(OpObra::AnotarGasto(Box::new(g)))]);
    assert_eq!(app.p.seguimiento.gastos.len(), 1);
    app.abrir_obra(PestanaObra::CosteReal);
    for _ in 0..2 {
        let mut out = ctx.run_ui(RawInput::default(), |ui| {
            let c = ui.ctx().clone();
            app.ventana_obra(&c);
        });
        out.textures_delta.clear();
    }
    let c = app.p.coste_real(app.gg, app.bi).unwrap();
    assert_eq!(c.total.real.material, dec!(500));
    app.aplicar(vec![Accion::Obra(OpObra::BorrarGasto(1))]);
    assert!(app.p.seguimiento.gastos.is_empty());
    app.deshacer();
    assert_eq!(app.p.seguimiento.gastos.len(), 1);
}

#[test]
fn descuento_comercial_y_contratar() {
    use obra::OpObra;
    let dir = std::env::temp_dir().join(format!("gui-contrato-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut app = Aplicacion::sin_ventana();
    app.fichero = Some(dir.join("obra.sqlite"));
    let (venta, _) = app.p.venta_y_descuento(app.gg, app.bi).unwrap();
    let d = ppto_core::venta::DescuentoComercial::cerrar_en(venta, ppto_core::redondear(venta - dec!(100), 0)).unwrap();
    app.aplicar(vec![Accion::Obra(OpObra::Descuento(Some(d)))]);
    let (v2, desc) = app.p.venta_y_descuento(app.gg, app.bi).unwrap();
    assert_eq!(v2, venta, "los precios no cambian");
    assert!(desc > dec!(99) && desc < dec!(101));
    // Se pinta el resumen y la ventana de contratar
    let ctx = Context::default();
    app.ver_contratar = true;
    app.pestana = Pestana::Resumen;
    for _ in 0..2 {
        let mut out = ctx.run_ui(RawInput::default(), |ui| app.pantalla(ui));
        out.textures_delta.clear();
    }
    // Contratar: bloquea ventas
    app.aplicar(vec![Accion::Obra(OpObra::Contratar(
        "01/11/2026".into(),
        app.gg,
        app.bi,
    ))]);
    let k = app.p.seguimiento.contrato.clone().unwrap();
    assert_eq!(k.importe, venta - desc);
    assert!(
        app.p
            .conceptos
            .values()
            .filter(|c| c.naturaleza == Naturaleza::Partida)
            .all(|c| c.venta_fija.is_some())
    );
    app.guardar(false);
    app.bloquear_revision_actual();
    assert!(app.revisiones.last().unwrap().bloqueada);
    // PDF de la oferta con la línea de descuento
    let o = app.opciones_pdf_obra();
    let datos = ppto_pdf::datos(&app.p, &o, None).unwrap();
    assert!(
        datos["descuento"]["concepto"]
            .as_str()
            .unwrap()
            .starts_with("Descuento comercial")
    );
    app.aplicar(vec![Accion::Obra(OpObra::AnularContrato)]);
    assert!(app.p.conceptos.values().all(|c| c.venta_fija.is_none()));
    let _ = std::fs::remove_dir_all(&dir);
}
