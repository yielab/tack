import unittest

import calc


class MultiplyTest(unittest.TestCase):
    def test_products(self):
        self.assertEqual(calc.multiply(3, 4), 12)
        self.assertEqual(calc.multiply(-2, 5), -10)
        self.assertEqual(calc.multiply(0, 9), 0)
        self.assertAlmostEqual(calc.multiply(0.5, 0.5), 0.25)

    def test_the_rest_still_works(self):
        self.assertEqual(calc.add(2, 3), 5)
        self.assertEqual(calc.divide(6, 3), 2)
