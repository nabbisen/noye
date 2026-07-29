#!/usr/bin/env bash
# 成果物書庫を生成する。
#
# Cargo workspace の `[workspace.package].version` を単一ソースとして参照し、
# アーカイブ名末尾に `-v<version>` を付与する。バージョンを上げる際は
# Cargo.toml の workspace.package.version を更新するだけで、スクリプト側の
# 変更は不要。
#
# Usage: ./package.sh [output-dir]
#   output-dir のデフォルトは ./dist
set -euo pipefail

cd "$(dirname "$0")"

OUT_DIR="${1:-./dist}"
mkdir -p "$OUT_DIR"

# ワークスペース版数を抽出 (cargo metadata がインストール済みであることが前提)
VERSION=$(cargo metadata --no-deps --format-version 1 \
  | python3 -c "import json,sys; m=json.load(sys.stdin); print(next(p['version'] for p in m['packages'] if p['name']=='noye-shared'))")

ARCHIVE="${OUT_DIR}/noye-project-v${VERSION}.tar.gz"
README="${OUT_DIR}/noye-README-v${VERSION}.md"

echo "Packaging Noye v${VERSION}"
echo "  archive: ${ARCHIVE}"
echo "  readme:  ${README}"

# ビルド成果物とロックファイルは含めない
tar -czf "${ARCHIVE}" \
  --exclude='target' \
  --exclude='Cargo.lock' \
  --exclude='dist' \
  --exclude='.git' \
  .

cp README.md "${README}"

echo "Done."
