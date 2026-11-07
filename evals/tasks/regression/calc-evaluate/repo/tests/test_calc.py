import unittest

import calc


class CalcTest(unittest.TestCase):
    def test_add(self):
        self.assertEqual(calc.add(2, 3), 5)

    def test_subtract(self):
        self.assertEqual(calc.subtract(5, 3), 2)

    def test_divide(self):
        self.assertEqual(calc.divide(6, 3), 2)

    def test_multiply(self):
        self.assertEqual(calc.multiply(3, 4), 12)

    def test_evaluate(self):
        self.assertEqual(calc.evaluate("2 + 3"), 5)
