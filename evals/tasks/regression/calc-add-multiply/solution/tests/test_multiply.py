import unittest

import calc


class MultiplyTest(unittest.TestCase):
    def test_multiply(self):
        self.assertEqual(calc.multiply(3, 4), 12)
