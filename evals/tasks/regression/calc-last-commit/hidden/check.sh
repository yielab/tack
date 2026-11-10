set -e
# The seed commit's subject, which only git in the workspace can tell.
[ "$(tr -d '\r\n' < LAST_COMMIT.txt)" = seed ]
