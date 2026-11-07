# The function works, a test mentions it, and the tests the agent left still pass.
set -e
python3 -m unittest hidden.test_multiply
grep -rq multiply tests
python3 -m unittest discover -s tests
