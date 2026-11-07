import unittest

import calc


class SubtractTest(unittest.TestCase):
    def test_differences(self):
        self.assertEqual(calc.subtract(10, 4), 6)
        self.assertEqual(calc.subtract(4, 10), -6)
        self.assertEqual(calc.subtract(0, 0), 0)
        self.assertAlmostEqual(calc.subtract(1.5, 0.5), 1.0)

    def test_the_rest_still_works(self):
        self.assertEqual(calc.add(2, 3), 5)
        self.assertEqual(calc.divide(6, 3), 2)
