#!/usr/bin/env bash
# 成果物書庫を生成する。
#
# Cargo workspace の `[workspace.package].version` を単一ソースとして参照し、
# その版数に一致する git タグからアーカイブを作る。作業ディレクトリではなく
# タグの追跡済みコンテンツだけを対象とするため、追跡外のファイルが混入する
# ことはなく、同じタグから何度作っても同一の書庫になる。
#
# 前提: バージョンに一致するタグが存在し、HEAD がそのタグを指しており、
# 作業ツリーがクリーンであること。いずれかを満たさない場合は書庫を作らず
# エラー終了する。
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

# タグは版数から導出する。HEAD から推測しない — 版数と食い違うタグへの
# チェックアウトを静かに取り違えないため。
TAG="${VERSION}"

DIRTY_STATUS="$(git status --porcelain)"
if [[ -n "$DIRTY_STATUS" ]]; then
  echo "error: working tree is dirty — refusing to build a release archive from an unreproducible state" >&2
  echo "$DIRTY_STATUS" >&2
  exit 1
fi

if ! git rev-parse -q --verify "refs/tags/${TAG}" >/dev/null; then
  echo "error: no tag '${TAG}' found for workspace version ${VERSION} — refusing to build an untagged release archive" >&2
  exit 1
fi

if [[ "$(git rev-parse HEAD)" != "$(git rev-parse "${TAG}^{commit}")" ]]; then
  echo "error: HEAD is not at tag '${TAG}' — refusing to build from a commit other than the tagged one" >&2
  exit 1
fi

ARCHIVE="${OUT_DIR}/noye-project-v${VERSION}.tar.gz"
README="${OUT_DIR}/noye-README-v${VERSION}.md"

echo "Packaging Noye v${VERSION}"
echo "  archive: ${ARCHIVE}"
echo "  readme:  ${README}"

# タグの追跡済みコンテンツだけを対象にする。作業ツリーの走査も除外リストも
# 不要 — 追跡されていないものは最初から対象に入らない。
git archive --format=tar.gz --prefix='' "${TAG}" -o "${ARCHIVE}"

cp README.md "${README}"

echo "Done."
