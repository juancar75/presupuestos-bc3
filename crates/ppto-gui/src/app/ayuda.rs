// SPDX-License-Identifier: GPL-3.0-or-later
//! Ayuda dentro del programa: el manual (`docs/manual.md`) con su índice,
//! búsqueda y apartados que se abren desde los botones «?».

use super::*;

/// El manual, el mismo que se publica (docs/manual.md).
pub(super) const MANUAL: &str = include_str!("../../../../docs/manual.md");

/// Un apartado del manual: título (sin «## ») y texto.
pub(super) struct Apartado {
    pub titulo: &'static str,
    pub texto: &'static str,
}

/// Los apartados `## …` del manual, en orden.
pub(super) fn apartados() -> Vec<Apartado> {
    let mut v = Vec::new();
    let mut resto = MANUAL;
    // Lo que va antes del primer «## » es el título del manual
    if let Some(i) = resto.find("\n## ") {
        resto = &resto[i + 1..];
    }
    while let Some(r) = resto.strip_prefix("## ") {
        let fin_titulo = r.find('\n').unwrap_or(r.len());
        let cuerpo = &r[fin_titulo..];
        let fin = cuerpo.find("\n## ").map_or(cuerpo.len(), |i| i + 1);
        v.push(Apartado {
            titulo: r[..fin_titulo].trim(),
            texto: cuerpo[..fin].trim(),
        });
        resto = &cuerpo[fin..];
    }
    v
}

/// Estado de la ventana de ayuda.
#[derive(Default)]
pub(super) struct Ayuda {
    pub abierta: bool,
    pub elegido: usize,
    pub buscar: String,
    /// Desplazar al principio del apartado al cambiarlo.
    pub arriba: bool,
}

impl Aplicacion {
    /// Abre la ayuda; con `tema`, en el primer apartado cuyo título lo contiene.
    pub(super) fn abrir_ayuda(&mut self, tema: Option<&str>) {
        self.ayuda.abierta = true;
        self.ayuda.arriba = true;
        if let Some(t) = tema {
            let t = super::valorar::plano(t);
            if let Some(i) = apartados()
                .iter()
                .position(|a| super::valorar::plano(a.titulo).contains(&t))
            {
                self.ayuda.elegido = i;
                self.ayuda.buscar.clear();
            }
        }
    }

    pub(super) fn ventana_ayuda(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F1)) {
            self.abrir_ayuda(None);
        }
        if !self.ayuda.abierta {
            return;
        }
        let ap = apartados();
        let a = &mut self.ayuda;
        a.elegido = a.elegido.min(ap.len().saturating_sub(1));
        let mut abierta = true;
        egui::Window::new("Ayuda · Manual de presupuestos-bc3")
            .open(&mut abierta)
            .default_size([860.0, 600.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Buscar:");
                    ui.add(
                        egui::TextEdit::singleline(&mut a.buscar)
                            .hint_text("p. ej. descuento, kit, Presto")
                            .desired_width(260.0),
                    );
                    if !a.buscar.is_empty() && ui.small_button("✖").clicked() {
                        a.buscar.clear();
                    }
                });
                ui.separator();
                let q = super::valorar::plano(&a.buscar);
                let visibles: Vec<usize> = (0..ap.len())
                    .filter(|&i| {
                        q.is_empty() || super::valorar::plano(&format!("{} {}", ap[i].titulo, ap[i].texto)).contains(&q)
                    })
                    .collect();
                if !q.is_empty()
                    && !visibles.contains(&a.elegido)
                    && let Some(&i) = visibles.first()
                {
                    a.elegido = i;
                    a.arriba = true;
                }
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(210.0);
                        egui::ScrollArea::vertical().id_salt("ayuda_indice").show(ui, |ui| {
                            for &i in &visibles {
                                if ui.selectable_label(a.elegido == i, ap[i].titulo).clicked() {
                                    a.elegido = i;
                                    a.arriba = true;
                                }
                            }
                            if visibles.is_empty() {
                                ui.label(RichText::new("Nada con esas palabras.").weak());
                            }
                        });
                    });
                    ui.separator();
                    let mut sa = egui::ScrollArea::vertical()
                        .id_salt("ayuda_texto")
                        .auto_shrink([false, false]);
                    if a.arriba {
                        sa = sa.vertical_scroll_offset(0.0);
                        a.arriba = false;
                    }
                    sa.show(ui, |ui| {
                        if let Some(x) = ap.get(a.elegido) {
                            ui.heading(x.titulo);
                            ui.add_space(4.0);
                            pintar_md(ui, x.texto, &q);
                        }
                    });
                });
            });
        self.ayuda.abierta = abierta;
    }
}

/// Texto con **negrita** y `código`; resalta `q` (ya en plano) si aparece.
fn linea(ui: &mut egui::Ui, s: &str, q: &str) {
    let mut job = egui::text::LayoutJob::default();
    let normal = egui::TextFormat {
        font_id: egui::TextStyle::Body.resolve(ui.style()),
        color: ui.visuals().text_color(),
        ..Default::default()
    };
    let resalta = !q.is_empty() && super::valorar::plano(s).contains(q);
    let mut fmt = normal.clone();
    if resalta {
        fmt.background = ui.visuals().selection.bg_fill.gamma_multiply(0.35);
    }
    let (mut negrita, mut codigo) = (false, false);
    let mut trozo = String::new();
    let volcar = |job: &mut egui::text::LayoutJob, t: &mut String, n: bool, c: bool| {
        if t.is_empty() {
            return;
        }
        let mut f = fmt.clone();
        if n {
            f.color = ui.visuals().strong_text_color();
        }
        if c {
            f.font_id = egui::TextStyle::Monospace.resolve(ui.style());
        }
        job.append(t, 0.0, f);
        t.clear();
    };
    let mut it = s.chars().peekable();
    while let Some(ch) = it.next() {
        if ch == '*' && it.peek() == Some(&'*') && !codigo {
            it.next();
            volcar(&mut job, &mut trozo, negrita, codigo);
            negrita = !negrita;
        } else if ch == '`' {
            volcar(&mut job, &mut trozo, negrita, codigo);
            codigo = !codigo;
        } else {
            trozo.push(ch);
        }
    }
    volcar(&mut job, &mut trozo, negrita, codigo);
    job.wrap.max_width = ui.available_width();
    ui.label(job);
}

/// Markdown sencillo del manual: párrafos, listas, listas numeradas y tablas.
fn pintar_md(ui: &mut egui::Ui, texto: &str, q: &str) {
    let lineas: Vec<&str> = texto.lines().collect();
    let mut i = 0;
    let mut tabla = 0;
    while i < lineas.len() {
        let l = lineas[i].trim_end();
        if l.trim().is_empty() {
            ui.add_space(6.0);
        } else if let Some(t) = l.strip_prefix("### ") {
            // Subapartado
            ui.add_space(8.0);
            ui.label(RichText::new(t.trim()).strong().size(15.0));
            ui.separator();
        } else if l.starts_with('|') {
            // Tabla: cabecera, separador y filas
            let mut filas: Vec<Vec<String>> = Vec::new();
            while i < lineas.len() && lineas[i].trim_start().starts_with('|') {
                let celdas: Vec<String> = lineas[i]
                    .trim()
                    .trim_matches('|')
                    .split('|')
                    .map(|c| c.trim().to_owned())
                    .collect();
                if !celdas.iter().all(|c| c.chars().all(|x| x == '-' || x == ':')) {
                    filas.push(celdas);
                }
                i += 1;
            }
            tabla += 1;
            egui::Grid::new(("ayuda_tabla", tabla))
                .striped(true)
                .spacing([14.0, 4.0])
                .show(ui, |ui| {
                    for (n, f) in filas.iter().enumerate() {
                        for c in f {
                            if n == 0 {
                                ui.label(RichText::new(c.replace("**", "")).strong());
                            } else {
                                linea(ui, c, q);
                            }
                        }
                        ui.end_row();
                    }
                });
            continue;
        } else if let Some(r) = l.strip_prefix("- ") {
            ui.horizontal_top(|ui| {
                ui.label("•");
                ui.vertical(|ui| linea(ui, r, q));
            });
        } else if let Some(r) = l.strip_prefix("  - ") {
            ui.horizontal_top(|ui| {
                ui.add_space(18.0);
                ui.label("◦");
                ui.vertical(|ui| linea(ui, r, q));
            });
        } else if let Some((n, r)) = l
            .split_once(". ")
            .filter(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
        {
            ui.horizontal_top(|ui| {
                ui.label(format!("{n}."));
                ui.vertical(|ui| linea(ui, r, q));
            });
        } else {
            linea(ui, l, q);
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_manual_tiene_sus_apartados() {
        let a = apartados();
        assert!(a.len() >= 10, "{}", a.len());
        assert!(a[0].titulo.starts_with("1. "));
        assert!(a.iter().any(|x| x.titulo.contains("Comprobar antes de valorar")));
        assert!(a.iter().all(|x| !x.texto.contains("\n## ")));
        // Cada subapartado «### » va en el apartado que le toca
        let venta = a.iter().find(|x| x.titulo.contains("Venta")).unwrap();
        assert!(!venta.texto.contains("### "), "en Venta solo va la venta");
        // Lo de los botones «?» existe
        for tema in [
            "comprobar",
            "valorar a mano",
            "carpeta de tarifas",
            "exportar",
            "claude",
            "certificaciones",
            "legalizacion",
        ] {
            let t = super::super::valorar::plano(tema);
            assert!(
                a.iter().any(|x| super::super::valorar::plano(x.titulo).contains(&t)),
                "{tema}"
            );
        }
    }
}
