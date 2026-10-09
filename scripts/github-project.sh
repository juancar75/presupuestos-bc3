#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Crea el tablero de GitHub Projects con los estados del proyecto y añade
# las Issues del backlog en su estado inicial.
#
# Requisitos: gh ≥ 2.40 autenticado con el permiso «project»
#   gh auth refresh -s project
# Uso:
#   scripts/github-project.sh OWNER REPO
set -euo pipefail

OWNER=${1:?OWNER}
REPO=${2:?REPO}
TITULO="presupuestos-bc3 — desarrollo"
ESTADOS="Backlog,Preparado,En desarrollo,Revisión técnica,Pruebas,Hecho"
RAIZ=$(cd "$(dirname "$0")/.." && pwd)

numero=$(gh project list --owner "$OWNER" --format json --jq ".projects[] | select(.title==\"$TITULO\") | .number" || true)
if [[ -z "$numero" ]]; then
  numero=$(gh project create --owner "$OWNER" --title "$TITULO" --format json --jq .number)
  echo "Proyecto creado: #$numero"
fi
gh project link "$numero" --owner "$OWNER" --repo "$OWNER/$REPO" >/dev/null 2>&1 || true
pid=$(gh project view "$numero" --owner "$OWNER" --format json --jq .id)

if ! gh project field-list "$numero" --owner "$OWNER" --format json --jq '.fields[].name' | grep -qx Estado; then
  gh project field-create "$numero" --owner "$OWNER" --name Estado \
    --data-type SINGLE_SELECT --single-select-options "$ESTADOS" >/dev/null
fi
campos=$(gh project field-list "$numero" --owner "$OWNER" --format json)
fid=$(jq -r '.fields[] | select(.name=="Estado") | .id' <<<"$campos")

jq -c '.tareas[] | {id, estado}' "$RAIZ/.github/backlog.json" | while read -r t; do
  id=$(jq -r .id <<<"$t"); estado=$(jq -r .estado <<<"$t")
  url=$(gh issue list -R "$OWNER/$REPO" --state all --search "$id in:title" --json title,url \
        --jq ".[] | select(.title | startswith(\"$id \")) | .url" | head -1)
  [[ -z "$url" ]] && { echo "  $id: Issue no encontrada (ejecuta antes github-bootstrap.py)"; continue; }
  item=$(gh project item-add "$numero" --owner "$OWNER" --url "$url" --format json --jq .id)
  oid=$(jq -r --arg e "$estado" '.fields[] | select(.name=="Estado") | .options[] | select(.name==$e) | .id' <<<"$campos")
  gh project item-edit --id "$item" --project-id "$pid" --field-id "$fid" --single-select-option-id "$oid" >/dev/null
  echo "  $id → $estado"
done
echo "Tablero: https://github.com/users/$OWNER/projects/$numero"
