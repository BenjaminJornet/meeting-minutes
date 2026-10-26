import unittest
from score import distance, words


class ScoringTests(unittest.TestCase):
    def test_normalization_keeps_accents_numbers_and_negation(self):
        self.assertEqual(words("Speaker É : n’a PAS validé 1 250 € !"),
                         ["speaker", "é", "n", "a", "pas", "validé", "1", "250"])

    def test_edit_distance(self):
        self.assertEqual(distance(words("non validé"), words("validé")), 1)
        self.assertEqual(distance(words("livrer vendredi"), words("livrer jeudi")), 1)
        self.assertEqual(distance([], words("hallucination")), 1)
        self.assertEqual(distance(words("bonjour"), words("bonjour")), 0)


if __name__ == "__main__":
    unittest.main()
