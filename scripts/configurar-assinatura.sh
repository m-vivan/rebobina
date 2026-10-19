#!/usr/bin/env bash
# Cadastra no GitHub os secrets que o workflow de release usa para assinar o app:
#   APPLE_CERTIFICATE           .p12 (certificado + chave privada) em base64
#   APPLE_CERTIFICATE_PASSWORD  senha do .p12
#   KEYCHAIN_PASSWORD           senha da keychain temporária do CI (gerada aqui)
#
# Envia SÓ a identidade de assinatura escolhida, mesmo que o chaveiro tenha
# outras (o `security export` do macOS sempre exporta todas juntas).
#
# Uso:  scripts/configurar-assinatura.sh [--dry-run]
#   --dry-run  gera e confere o .p12, mas não envia nada ao GitHub.

set -euo pipefail

REPO="${REPO:-m-vivan/rebobina}"
IDENTITY_PREFIX="${IDENTITY_PREFIX:-Apple Development}"
OPENSSL=/usr/bin/openssl # LibreSSL do macOS: lê o .p12 exportado pelo chaveiro sem -legacy
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"
DRY_RUN=0
[ "${1:-}" = "--dry-run" ] && DRY_RUN=1

fail() { echo "Erro: $*" >&2; exit 1; }

tmp="$(mktemp -d)"
chmod 700 "$tmp"
trap 'rm -rf "$tmp"' EXIT

# 1) Identidade de assinatura
identity="$(security find-identity -v -p codesigning "$KEYCHAIN" \
  | sed -n "s/.*\"\(${IDENTITY_PREFIX}[^\"]*\)\".*/\1/p" | head -1)"
[ -n "$identity" ] || fail "nenhuma identidade \"$IDENTITY_PREFIX\" válida no chaveiro. Veja CONTRIBUTING.md."
echo "Identidade: $identity"

# 2) GitHub
if [ "$DRY_RUN" -eq 0 ]; then
  command -v gh >/dev/null || fail "instale o GitHub CLI (brew install gh)."
  admin="$(gh api "repos/$REPO" -q .permissions.admin 2>/dev/null || true)"
  [ "$admin" = "true" ] || fail "a conta ativa do gh não administra $REPO (troque com: gh auth switch)."
fi

# 3) Senha do .p12
read -r -s -p "Crie uma senha para o certificado exportado: " p12_pass; echo
read -r -s -p "Repita a senha: " p12_pass2; echo
[ -n "$p12_pass" ] || fail "a senha não pode ser vazia."
[ "$p12_pass" = "$p12_pass2" ] || fail "as senhas não conferem."

# 4) Exporta todas as identidades (o macOS pode pedir para liberar cada chave)
export_pass="$($OPENSSL rand -hex 16)"
echo "Exportando do chaveiro (se o macOS pedir, digite a senha do Mac e clique em Permitir)..."
security export -k "$KEYCHAIN" -t identities -f pkcs12 -P "$export_pass" -o "$tmp/todas.p12" >/dev/null
$OPENSSL pkcs12 -in "$tmp/todas.p12" -passin "pass:$export_pass" -nodes -out "$tmp/todas.pem" 2>/dev/null

# 5) Separa só o certificado escolhido e a chave com o mesmo localKeyID
python3 - "$tmp/todas.pem" "$identity" "$tmp/cert.pem" "$tmp/chave.pem" <<'PY'
import re, sys
pem_path, identity, cert_out, key_out = sys.argv[1:]
text = open(pem_path).read()
blocks = re.findall(r"(Bag Attributes.*?-----END [A-Z ]+-----)", text, re.S)
def key_id(block):
    m = re.search(r"localKeyID:\s*([0-9A-F ]+)", block)
    return m.group(1).strip() if m else None
certs = [b for b in blocks if "BEGIN CERTIFICATE" in b and identity in b]
if len(certs) != 1:
    sys.exit(f"esperava 1 certificado '{identity}', achei {len(certs)}")
kid = key_id(certs[0])
keys = [b for b in blocks if "PRIVATE KEY" in b and kid and key_id(b) == kid]
if len(keys) != 1:
    sys.exit("chave privada do certificado não encontrada")
strip = lambda b: b[b.index("-----BEGIN"):] + "\n"
open(cert_out, "w").write(strip(certs[0]))
open(key_out, "w").write(strip(keys[0]))
PY

# 6) Novo .p12 só com essa identidade
$OPENSSL pkcs12 -export -in "$tmp/cert.pem" -inkey "$tmp/chave.pem" -name "$identity" \
  -passout "pass:$p12_pass" -out "$tmp/assinatura.p12"

# Confere: um único certificado, o certo, e a senha abre o arquivo
subjects="$($OPENSSL pkcs12 -in "$tmp/assinatura.p12" -passin "pass:$p12_pass" -nokeys 2>/dev/null | grep -c '^subject=' || true)"
$OPENSSL pkcs12 -in "$tmp/assinatura.p12" -passin "pass:$p12_pass" -nokeys 2>/dev/null \
  | grep -q "$identity" || fail "o .p12 gerado não contém $identity"
[ "$subjects" = "1" ] || fail "o .p12 gerado tem $subjects certificados (esperado 1)"
echo "Certificado exportado e conferido (só \"$identity\")."

if [ "$DRY_RUN" -eq 1 ]; then
  echo "--dry-run: nada foi enviado ao GitHub."
  exit 0
fi

# 7) Secrets
base64 -i "$tmp/assinatura.p12" | gh secret set APPLE_CERTIFICATE --repo "$REPO"
printf '%s' "$p12_pass" | gh secret set APPLE_CERTIFICATE_PASSWORD --repo "$REPO"
$OPENSSL rand -hex 16 | gh secret set KEYCHAIN_PASSWORD --repo "$REPO"

echo
gh secret list --repo "$REPO"
echo
echo "Pronto. A próxima tag v* gera a release assinada."
