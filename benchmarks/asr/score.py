"""WER with a fixed normalization, identical for every ASR candidate."""
import argparse
import json
from pathlib import Path
import re
import unicodedata


def words(text):
    return re.findall(r"\w+", unicodedata.normalize("NFC", text).lower().replace("’", "'"))


def distance(reference, hypothesis):
    previous = list(range(len(hypothesis) + 1))
    for i, ref in enumerate(reference, 1):
        current = [i]
        for j, hyp in enumerate(hypothesis, 1):
            current.append(min(current[-1] + 1, previous[j] + 1, previous[j - 1] + (ref != hyp)))
        previous = current
    return previous[-1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("results", type=Path)
    parser.add_argument("references", type=Path, help="one corrected .txt per WAV, same stem")
    args = parser.parse_args()
    totals = {}
    for path in sorted(args.results.glob("*.json")):
        result = json.loads(path.read_text())
        if "text" not in result:
            continue
        reference = args.references / (Path(result["audio"]).stem + ".txt")
        if not reference.is_file():
            raise SystemExit(f"Missing reference: {reference}")
        ref, hyp = words(reference.read_text()), words(result["text"])
        if not ref:
            raise SystemExit(f"Empty reference: {reference}")
        errors = distance(ref, hyp)
        total = totals.setdefault(result["engine"], [0, 0])
        total[0] += errors
        total[1] += len(ref)
        print(f"engine={result['engine']} | audio={result['audio']!r} | wer={errors/len(ref):.4f}")
    for engine, (errors, count) in totals.items():
        print(f"engine={engine} | corpus_wer={errors/count:.4f} | reference_words={count}")


if __name__ == "__main__":
    main()
