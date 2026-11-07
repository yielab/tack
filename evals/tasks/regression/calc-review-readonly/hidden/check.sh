# Nothing the agent was given changed, and nothing new was left behind.
set -e
sha256sum -c hidden/sums.txt
test "$(find . -path ./hidden -prune -o -type f ! -name '.*' -print | wc -l)" = 3
