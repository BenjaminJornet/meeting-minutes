"""Compare RTTM speaker annotations with fixed DER collars and optimal mapping."""
import argparse
from pathlib import Path


def main():
    from pyannote.database.util import load_rttm
    from pyannote.metrics.diarization import DiarizationErrorRate
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("hypothesis", type=Path)
    parser.add_argument("reference", type=Path)
    args = parser.parse_args()
    hypothesis = load_rttm(str(args.hypothesis))
    reference = load_rttm(str(args.reference))
    if set(hypothesis) != set(reference):
        raise SystemExit("References and hypotheses must cover exactly the same audio files.")
    for collar in (0, 0.25):
        metric = DiarizationErrorRate(collar=collar, skip_overlap=False)
        for uri, annotation in reference.items():
            score = metric(annotation, hypothesis[uri])
            print(f"audio={uri!r} | collar={collar} | der={score:.4f}")
        print(f"corpus_der={abs(metric):.4f} | collar={collar}")


if __name__ == "__main__":
    main()
