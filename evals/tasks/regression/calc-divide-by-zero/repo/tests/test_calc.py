import unittest

import calc


class CalcTest(unittest.TestCase):
    def test_add(self):
        self.assertEqual(calc.add(2, 3), 5)

    def test_subtract(self):
        self.assertEqual(calc.subtract(5, 3), 2)

    def test_divide(self):
        self.assertEqual(calc.divide(6, 3), 2)

    def test_divide_by_zero(self):
        with self.assertRaises(ValueError) as caught:
            calc.divide(1, 0)
        self.assertEqual(str(caught.exception), "cannot divide by zero")
