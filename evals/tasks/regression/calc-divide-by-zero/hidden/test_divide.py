import unittest

import calc


class DivideTest(unittest.TestCase):
    def test_zero_divisor_is_a_value_error(self):
        for numerator in (1, 0, -4, 2.5):
            with self.assertRaises(ValueError) as caught:
                calc.divide(numerator, 0)
            self.assertEqual(str(caught.exception), "cannot divide by zero")

    def test_other_divisions_are_untouched(self):
        self.assertEqual(calc.divide(6, 3), 2)
        self.assertEqual(calc.divide(1, 2), 0.5)
        self.assertEqual(calc.divide(0, 5), 0)
        self.assertEqual(calc.divide(-8, 2), -4)
