set -e
grep -q '^## Unreleased' CHANGELOG.md
for name in add subtract divide; do
  grep -qi "$name" CHANGELOG.md
done
