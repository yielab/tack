import unittest

import calc


class AddTest(unittest.TestCase):
    def test_sums(self):
        self.assertEqual(calc.add(0, 5), 5)
        self.assertEqual(calc.add(-2, -3), -5)
        self.assertEqual(calc.add(7, -7), 0)
        self.assertAlmostEqual(calc.add(0.5, 0.25), 0.75)

    def test_the_rest_still_works(self):
        self.assertEqual(calc.subtract(5, 3), 2)
        self.assertEqual(calc.divide(6, 3), 2)
