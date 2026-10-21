#!/usr/bin/env bash
# Configura a atualização automática do app (plugin updater do Tauri):
#   1. gera o par de chaves de atualização em ~/.tauri/ (se ainda não existir);
#   2. cadastra no GitHub os secrets TAURI_SIGNING_PRIVATE_KEY e
#      TAURI_SIGNING_PRIVATE_KEY_PASSWORD, usados pelo workflow de release;
#   3. grava a chave PÚBLICA em apps/desktop/src-tauri/tauri.conf.json.
#
# A chave privada assina cada atualização; o app só instala o que bate com a
# chave pública. Guarde a chave privada (e a senha) num lugar seguro: sem ela,
# não dá para publicar atualizações que as versões instaladas aceitem.
#
# Uso:  scripts/configurar-atualizador.sh [--dry-run]
#   --dry-run  gera a chave e grava a pública, mas não envia nada ao GitHub.

set -euo pipefail

REPO="${REPO:-m-vivan/rebobina}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/apps/desktop"
CONF="$APP/src-tauri/tauri.conf.json"
KEY="${KEY:-$HOME/.tauri/rebobina-updater.key}"
DRY_RUN=0
[ "${1:-}" = "--dry-run" ] && DRY_RUN=1

fail() { echo "Erro: $*" >&2; exit 1; }

if [ "$DRY_RUN" -eq 0 ]; then
  command -v gh >/dev/null || fail "instale o GitHub CLI (brew install gh)."
  admin="$(gh api "repos/$REPO" -q .permissions.admin 2>/dev/null || true)"
  [ "$admin" = "true" ] || fail "a conta ativa do gh não administra $REPO (troque com: gh auth switch)."
fi

# 1) Par de chaves
if [ -f "$KEY" ] && [ -f "$KEY.pub" ]; then
  echo "Usando a chave existente: $KEY"
  read -r -s -p "Senha da chave de atualização: " pass; echo
else
  read -r -s -p "Crie uma senha para a chave de atualização: " pass; echo
  read -r -s -p "Repita a senha: " pass2; echo
  [ -n "$pass" ] || fail "a senha não pode ser vazia."
  [ "$pass" = "$pass2" ] || fail "as senhas não conferem."
  mkdir -p "$(dirname "$KEY")"
  (cd "$APP" && npx tauri signer generate --ci -w "$KEY" -p "$pass" >/dev/null)
  chmod 600 "$KEY"
  echo "Chave criada em $KEY (guarde um backup dela e da senha)."
fi
[ -f "$KEY.pub" ] || fail "chave pública não encontrada em $KEY.pub"

# 2) Grava a chave pública na configuração do app
python3 - "$CONF" "$KEY.pub" <<'PY'
import json, re, sys
conf_path, pub_path = sys.argv[1:]
pub = open(pub_path).read().strip()
text = open(conf_path).read()
new, n = re.subn(r'("pubkey":\s*)"[^"]*"', lambda m: m.group(1) + json.dumps(pub), text, count=1)
if n != 1:
    sys.exit("campo plugins.updater.pubkey não encontrado no tauri.conf.json")
json.loads(new)  # confere que continua um JSON válido
open(conf_path, "w").write(new)
PY
echo "Chave pública gravada em apps/desktop/src-tauri/tauri.conf.json."

if [ "$DRY_RUN" -eq 1 ]; then
  echo "--dry-run: nada foi enviado ao GitHub."
  exit 0
fi

# 3) Secrets
gh secret set TAURI_SIGNING_PRIVATE_KEY --repo "$REPO" < "$KEY"
printf '%s' "$pass" | gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --repo "$REPO"

echo
gh secret list --repo "$REPO"
echo
echo "Pronto. Falta só publicar: faça commit do tauri.conf.json (chave pública) e gere uma release."
