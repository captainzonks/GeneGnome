#!/usr/bin/env python3
# ==============================================================================
# smoke_test.py - End-to-End Check of the Demo Stack
# ==============================================================================
# Description: Submits the synthetic files through the web proxy, reads the
#              download credentials from the captured email, downloads the
#              results and compares every user call with the expected merge.
#              Standard library only.
# Author: Matt Barham
# Created: 2026-11-26
# Modified: 2026-11-26
# Version: 1.0.0
# ==============================================================================

import collections
import io
import json
import re
import sqlite3
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
import zipfile
from pathlib import Path

BASE = "http://proxy"
API = f"{BASE}/api/genetics"
MAIL = "http://mailpit:8025/api/v1"
INPUTS = Path("/data")
EMAIL = "demo@example.com"
JOB_TIMEOUT_SECONDS = 600


def request(url: str, data: bytes | None = None, headers: dict | None = None):
    """Return (status, body), without raising on HTTP error statuses."""
    call = urllib.request.Request(url, data=data, headers=headers or {})
    try:
        with urllib.request.urlopen(call, timeout=120) as response:
            return response.status, response.read()
    except urllib.error.HTTPError as error:
        return error.code, error.read()


def get_json(url: str):
    status, body = request(url)
    assert status == 200, f"GET {url} returned {status}: {body[:300]!r}"
    return json.loads(body)


def wait_for(description: str, probe, seconds: int):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        try:
            result = probe()
        except (OSError, AssertionError):
            result = None
        if result:
            return result
        time.sleep(1)
    raise AssertionError(f"Timed out waiting for {description}")


def multipart(fields: list[tuple[str, str]], files: list[tuple[str, Path]]):
    boundary = uuid.uuid4().hex
    body = io.BytesIO()
    for name, value in fields:
        body.write(
            f"--{boundary}\r\nContent-Disposition: form-data; "
            f'name="{name}"\r\n\r\n{value}\r\n'.encode()
        )
    for name, path in files:
        body.write(
            f"--{boundary}\r\nContent-Disposition: form-data; "
            f'name="{name}"; filename="{path.name}"\r\n'
            "Content-Type: application/octet-stream\r\n\r\n".encode()
        )
        body.write(path.read_bytes())
        body.write(b"\r\n")
    body.write(f"--{boundary}--\r\n".encode())
    return body.getvalue(), {
        "Content-Type": f"multipart/form-data; boundary={boundary}"
    }


def submit_job() -> str:
    files = [("genome_file", INPUTS / "genome_demo.txt")]
    files += [("vcf_files", INPUTS / f"chr{c}.dose.vcf.gz") for c in range(1, 23)]
    files.append(("pgs_file", INPUTS / "scores.txt"))
    fields = [
        ("user_email", EMAIL),
        ("output_formats", "sqlite"),
        ("output_formats", "vcf"),
    ]
    body, headers = multipart(fields, files)
    status, response = request(f"{API}/jobs", body, headers)
    assert status == 200, f"Submission returned {status}: {response[:300]!r}"
    return json.loads(response)["job_id"]


def job_finished(job_id: str):
    status = get_json(f"{API}/jobs/{job_id}/status")["status"]
    return status if status in ("completed", "failed") else None


def credentials_from_email(job_id: str):
    for summary in get_json(f"{MAIL}/messages")["messages"]:
        text = get_json(f"{MAIL}/message/{summary['ID']}")["Text"]
        if job_id not in text:
            continue
        password = re.search(r"Download Password: (\S+)", text)
        token = re.search(r"/download\?token=(\S+)", text)
        assert password and token, "Email lacks a password or download link"
        return token.group(1), password.group(1)
    return None


def check_results(archive: bytes) -> None:
    expected = {
        (chromosome, position): (dosage, source)
        for chromosome, position, dosage, source in json.loads(
            (INPUTS / "expected.json").read_text()
        )
    }
    with zipfile.ZipFile(io.BytesIO(archive)) as results:
        names = results.namelist()
        assert "pgs_scores.csv" in names, f"No score table in {names}"
        assert any(n.endswith(".vcf.gz") for n in names), f"No VCF in {names}"
        database = next(n for n in names if n.endswith(".db"))
        with tempfile.TemporaryDirectory() as directory:
            results.extract(database, directory)
            connection = sqlite3.connect(Path(directory) / database)
            actual = {
                (chromosome, position): (dosage, source)
                for chromosome, position, dosage, source in connection.execute(
                    "SELECT chromosome, position, dosage, source FROM variants "
                    "WHERE sample_id = 'samp51'"
                )
            }
            connection.close()

    assert (
        actual.keys() == expected.keys()
    ), f"{len(actual)} variants in the results, {len(expected)} expected"
    wrong = []
    for key, (dosage, source) in expected.items():
        got_dosage, got_source = actual[key]
        same_dosage = (
            got_dosage is None
            if dosage is None
            else got_dosage is not None and abs(got_dosage - dosage) < 1e-3
        )
        if not same_dosage or got_source != source:
            wrong.append((key, (dosage, source), actual[key]))
    assert not wrong, f"{len(wrong)} calls differ, for example {wrong[:5]}"

    counts = collections.Counter(source for _, source in expected.values())
    print(f"All {len(expected)} user calls match: {dict(counts)}")


def main() -> None:
    wait_for("the API", lambda: request(f"{API}/ready")[0] == 200, 120)
    job_id = submit_job()
    print(f"Submitted job {job_id}")

    status = wait_for("the job", lambda: job_finished(job_id), JOB_TIMEOUT_SECONDS)
    assert status == "completed", f"Job {status}"

    token, password = wait_for(
        "the download email", lambda: credentials_from_email(job_id), 60
    )
    query = urllib.parse.urlencode({"token": token, "password": "not-the-password"})
    assert request(f"{API}/download?{query}")[0] == 400, "Wrong password accepted"

    query = urllib.parse.urlencode({"token": token, "password": password})
    visualization = get_json(f"{API}/visualization?{query}")
    assert visualization["chromosomes_processed"] == 22

    code, archive = request(f"{API}/download?{query}")
    assert code == 200, f"Download returned {code}: {archive[:300]!r}"
    check_results(archive)
    print("Demo smoke test passed")


if __name__ == "__main__":
    main()
