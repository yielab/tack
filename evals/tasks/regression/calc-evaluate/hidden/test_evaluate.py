import unittest

import calc


class EvaluateTest(unittest.TestCase):
    def test_each_operator(self):
        self.assertEqual(calc.evaluate("2 + 3"), 5)
        self.assertEqual(calc.evaluate("5 - 8"), -3)
        self.assertEqual(calc.evaluate("6 * 7"), 42)
        self.assertEqual(calc.evaluate("10 / 4"), 2.5)

    def test_decimals(self):
        self.assertAlmostEqual(calc.evaluate("1.5 + 2"), 3.5)

    def test_an_unknown_operator_is_a_value_error(self):
        with self.assertRaises(ValueError):
            calc.evaluate("2 ^ 3")

    def test_dividing_by_zero_is_what_divide_does(self):
        with self.assertRaises(ZeroDivisionError):
            calc.evaluate("1 / 0")
