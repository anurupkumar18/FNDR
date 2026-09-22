import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import decision_calibration as dc


class SoftmaxTests(unittest.TestCase):
    def test_sums_to_one_and_temperature_flattens(self):
        p1 = dc.softmax([4.0, 0.0])
        p2 = dc.softmax([4.0, 0.0], temperature=4.0)
        self.assertAlmostEqual(sum(p1), 1.0)
        self.assertGreater(p1[0], p2[0])
        self.assertAlmostEqual(dc.softmax([1.0, 1.0])[0], 0.5)

    def test_large_logits_do_not_overflow(self):
        self.assertAlmostEqual(dc.softmax([1000.0, 0.0])[0], 1.0)


class CalibrationTests(unittest.TestCase):
    def setUp(self):
        self.rows = [[4.0, 0.0]] * 10
        self.labels = [0, 0, 0, 0, 0, 0, 1, 1, 1, 1]

    def test_fit_temperature_softens_an_overconfident_model(self):
        temperature = dc.fit_temperature(self.rows, self.labels)
        self.assertGreater(temperature, 1.5)
        self.assertLess(dc.nll(self.rows, self.labels, temperature), dc.nll(self.rows, self.labels, 1.0))

    def test_ece_drops_after_temperature_scaling(self):
        confs_before, correct_before = dc.confidences_and_correctness(self.rows, self.labels, 1.0)
        temperature = dc.fit_temperature(self.rows, self.labels)
        confs_after, correct_after = dc.confidences_and_correctness(self.rows, self.labels, temperature)
        self.assertGreater(dc.expected_calibration_error(confs_before, correct_before), 0.3)
        self.assertLess(dc.expected_calibration_error(confs_after, correct_after), 0.05)

    def test_ece_is_zero_when_confidence_matches_accuracy(self):
        self.assertAlmostEqual(dc.expected_calibration_error([1.0, 1.0], [True, True]), 0.0)
        self.assertAlmostEqual(dc.expected_calibration_error([0.9] * 10, [True] * 5 + [False] * 5), 0.4)


class ThresholdTests(unittest.TestCase):
    confs = [0.99, 0.95, 0.90, 0.60, 0.55]
    correct = [True, True, True, False, True]

    def test_strict_target_accepts_only_the_confident_prefix(self):
        threshold, coverage, precision = dc.threshold_for_precision(self.confs, self.correct, 1.0)
        self.assertEqual((threshold, coverage, precision), (0.90, 0.6, 1.0))

    def test_looser_target_buys_more_coverage(self):
        threshold, coverage, precision = dc.threshold_for_precision(self.confs, self.correct, 0.8)
        self.assertEqual((threshold, coverage, precision), (0.55, 1.0, 0.8))

    def test_unreachable_target_returns_none(self):
        self.assertIsNone(dc.threshold_for_precision([0.9, 0.8], [False, False], 0.9))

    def test_ties_are_not_split(self):
        result = dc.threshold_for_precision([0.9, 0.9, 0.5], [True, False, True], 0.5)
        self.assertEqual(result[1], 1.0)


class RouteTests(unittest.TestCase):
    def test_three_way_routing(self):
        self.assertEqual(dc.route(0.97, 0.9, 0.6), "automate")
        self.assertEqual(dc.route(0.7, 0.9, 0.6), "review")
        self.assertEqual(dc.route(0.3, 0.9, 0.6), "human")
        self.assertEqual(dc.route(0.9, 0.9, 0.6), "automate")

    def test_inverted_thresholds_are_rejected(self):
        with self.assertRaises(ValueError):
            dc.route(0.5, 0.6, 0.9)


if __name__ == "__main__":
    unittest.main()
