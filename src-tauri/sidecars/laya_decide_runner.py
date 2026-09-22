#!/usr/bin/env python3
"""Run one Laya prediction from a JSON state and question payload."""

import json
import sys

from laya import Router


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: laya_decide_runner.py '<json payload>'", file=sys.stderr)
        return 1
    try:
        payload = json.loads(sys.argv[1])
        state = payload["state"]
        questions = payload["questions"]
    except (json.JSONDecodeError, KeyError, TypeError) as error:
        print(f"invalid Laya payload: {error}", file=sys.stderr)
        return 2

    router = Router(preload=True)
    result = router.predict(state, questions)
    print(json.dumps(result))
    return 0


if __name__ == "__main__":
    sys.exit(main())
