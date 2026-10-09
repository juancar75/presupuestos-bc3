#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Crea etiquetas, hitos e Issues del backlog en GitHub (API REST vía `gh`).

Idempotente: si una etiqueta, hito o Issue (por prefijo «P-NNN») ya existe,
lo actualiza o lo deja como está.

Uso:  python3 scripts/github-bootstrap.py OWNER/REPO [--dry-run] [--proteger-main]
"""
import json
import subprocess
import sys
from pathlib import Path

RAIZ = Path(__file__).resolve().parent.parent

ETIQUETAS = {
    "arquitectura": ("1d76db", "Estructura, módulos y decisiones técnicas"),
    "costes": ("0e8a16", "Precios, costes, márgenes y redondeos"),
    "mediciones": ("5319e7", "Mediciones y unidades"),
    "bc3": ("b60205", "Intercambio FIEBDC-3 / BC3 y Presto"),
    "versiones": ("c5def5", "Revisiones y variantes de presupuesto"),
    "interfaz": ("fbca04", "Aplicación de escritorio"),
    "informes": ("f9d0c4", "Informes PDF y Excel"),
    "subcontratas": ("006b75", "Paquetes de trabajo y subcontratación"),
    "mano-de-obra": ("0052cc", "Horas, oficios y rendimientos"),
    "compras": ("bfdadc", "Proveedores, ofertas y adjudicaciones"),
    "mcp": ("7057ff", "Servidor Model Context Protocol"),
    "seguridad": ("d93f0b", "Seguridad, permisos y auditoría"),
    "expertis": ("8b4513", "Integración con eXpertis"),
    "documentacion": ("0075ca", "Documentación y publicación"),
    "P0-critical": ("b60205", "Bloqueante"),
    "P1-high": ("d93f0b", "Prioridad alta"),
    "P2-medium": ("fbca04", "Prioridad normal"),
    "P3-low": ("c2e0c6", "Mejora no urgente"),
    "good-first-issue": ("7057ff", "Adecuada para nuevos colaboradores"),
    "needs-engineering-review": ("e99695", "Requiere validación por un ingeniero"),
}

DEFINICION_HECHO = """### Definición de hecho
- [ ] Criterio de aceptación demostrado
- [ ] Pruebas automáticas (casos normales, límites y errores)
- [ ] Valores económicos calculados a mano y documentados (si aplica)
- [ ] Revisión técnica de ingeniería y presupuestación (si aplica)
- [ ] Documentación y CHANGELOG actualizados"""

DRY = "--dry-run" in sys.argv


def gh(*args, datos=None):
    cmd = ["gh", "api", *args]
    if DRY and any(a in ("POST", "PATCH") for a in args):
        print("DRY", " ".join(cmd), (datos or {}).get("title") or (datos or {}).get("name") or "")
        return {}
    r = subprocess.run(cmd, input=json.dumps(datos) if datos else None, capture_output=True, text=True)
    if r.returncode != 0:
        raise RuntimeError(f"{' '.join(cmd)}\n{r.stdout}\n{r.stderr}")
    return json.loads(r.stdout) if r.stdout.strip() else {}


def paginado(ruta):
    out, page = [], 1
    while True:
        lote = gh(f"{ruta}{'&' if '?' in ruta else '?'}per_page=100&page={page}")
        out += lote
        if len(lote) < 100:
            return out
        page += 1


def cuerpo(t):
    lineas = [f"**{t['id']}** · Hito {t['hito']}", "", "### Alcance"]
    lineas += [f"- {p}" if not p[:1].isdigit() else f"  {p}" for p in t["puntos"]]
    lineas += ["", "### Criterio de aceptación", t["aceptacion"], ""]
    if t.get("avance"):
        lineas += ["### Avance", t["avance"], ""]
    lineas += [DEFINICION_HECHO, "", "_Generada desde `.github/backlog.json`._"]
    return "\n".join(lineas)


def main():
    if len(sys.argv) < 2 or "/" not in sys.argv[1]:
        sys.exit(__doc__)
    repo = sys.argv[1]
    backlog = json.loads((RAIZ / ".github/backlog.json").read_text(encoding="utf-8"))

    existentes = {e["name"] for e in paginado(f"repos/{repo}/labels")}
    for nombre, (color, desc) in ETIQUETAS.items():
        datos = {"name": nombre, "color": color, "description": desc}
        if nombre in existentes:
            gh("--method", "PATCH", f"repos/{repo}/labels/{nombre}", "--input", "-", datos=datos)
        else:
            gh("--method", "POST", f"repos/{repo}/labels", "--input", "-", datos=datos)
    print(f"Etiquetas: {len(ETIQUETAS)}")

    hitos = {m["title"].split(" ")[0]: m["number"] for m in paginado(f"repos/{repo}/milestones?state=all")}
    for clave, nombre in backlog["hitos"].items():
        if clave not in hitos:
            m = gh("--method", "POST", f"repos/{repo}/milestones", "--input", "-",
                   datos={"title": f"{clave} — {nombre}", "description": f"Hito {clave} del backlog inicial."})
            hitos[clave] = m.get("number")
    print(f"Hitos: {len(backlog['hitos'])}")

    issues = paginado(f"repos/{repo}/issues?state=all")
    por_id = {i["title"].split(" ")[0]: i for i in issues if "pull_request" not in i}
    creadas = 0
    for t in backlog["tareas"]:
        etiquetas = [t["etiqueta"], t["prioridad"]] + (["needs-engineering-review"] if t["revision_tecnica"] else [])
        datos = {"title": f"{t['id']} {t['titulo']}", "body": cuerpo(t), "labels": etiquetas,
                 "milestone": hitos.get(t["hito"])}
        if t["id"] in por_id:
            gh("--method", "PATCH", f"repos/{repo}/issues/{por_id[t['id']]['number']}", "--input", "-", datos=datos)
        else:
            gh("--method", "POST", f"repos/{repo}/issues", "--input", "-", datos=datos)
            creadas += 1
    print(f"Issues: {creadas} creadas, {len(backlog['tareas']) - creadas} actualizadas")

    if "--proteger-main" in sys.argv:
        gh("--method", "PUT", f"repos/{repo}/branches/main/protection", "--input", "-", datos={
            "required_status_checks": {"strict": True, "contexts": [
                "Pruebas (ubuntu-latest)", "Pruebas (windows-latest)", "Licencias y avisos de seguridad"]},
            "enforce_admins": False,
            "required_pull_request_reviews": {"required_approving_review_count": 1,
                                              "require_code_owner_reviews": True},
            "restrictions": None,
            "allow_force_pushes": False,
            "allow_deletions": False,
        })
        print("Rama main protegida: PR obligatoria, 1 revisión, CODEOWNERS y CI en verde")


if __name__ == "__main__":
    main()
