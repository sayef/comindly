"""Re-drop a vendor file into the incoming prefix after a fix, so lab-ingest picks it up again.

Usage: python tools/redrop.py <local-file> <vendor>/<file-name>
"""
import sys

import boto3

LAB_DROPS_BUCKET = "northvile-prod-lab-drops"

s3 = boto3.client("s3")


def redrop(path: str, name: str) -> str:
    key = f"incoming/{name}"
    with open(path, "rb") as fh:
        s3.put_object(Bucket=LAB_DROPS_BUCKET, Key=key, Body=fh.read(), ContentType="text/csv")
    return key


if __name__ == "__main__":
    print(redrop(sys.argv[1], sys.argv[2]))
