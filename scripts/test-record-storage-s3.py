#!/usr/bin/env python3
"""Run selected product focals against isolated Moto S3 and temporary STS keys.

Install the test dependencies from scripts/record-storage-test-requirements.txt.
Pass the graphit-app unit-test executable emitted by Cargo with --test-binary.
This checks consumer contracts; Moto is not a qualification of production S3 IAM.
"""
import argparse
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--test-binary", type=Path, required=True)
    parser.add_argument("--filter", default="task_server::record_storage_tests::",
                        help="Rust test filter; select only affected consumer focals")
    parser.add_argument("--require-parquet-ranges", action="store_true",
                        help="assert Icebug query ranges; verify the deliberate corruption fixture separately")
    args = parser.parse_args()
    binary = args.test_binary.resolve(strict=True)
    try:
        import boto3
        import moto  # noqa: F401
    except ImportError:
        parser.error("install scripts/record-storage-test-requirements.txt in a test virtualenv")
    with tempfile.TemporaryDirectory(prefix="graphit-record-storage-") as temporary:
        root = Path(temporary)
        with socket.socket() as selected:
            selected.bind(("127.0.0.1", 0))
            port = selected.getsockname()[1]
        endpoint = f"http://127.0.0.1:{port}"
        range_log = root / "parquet-ranges.jsonl"
        # Observe the actual S3 HTTP boundary, without recording credentials.
        server_command = [sys.executable, "-m", "moto.server", "-H", "127.0.0.1", "-p", str(port)]
        if args.require_parquet_ranges:
            server_command = [sys.executable, "-c", r'''
import json, sys
from moto.server import DomainDispatcherApplication, create_backend_app
from werkzeug.serving import run_simple
backend = DomainDispatcherApplication(create_backend_app)
def app(environ, start_response):
    if environ.get("REQUEST_METHOD") in ("GET", "PUT") and environ.get("PATH_INFO", "").endswith(".parquet"):
        with open(sys.argv[2], "a") as log:
            log.write(json.dumps({"method": environ["REQUEST_METHOD"], "path": environ["PATH_INFO"],
                                  "range": environ.get("HTTP_RANGE"), "size": environ.get("CONTENT_LENGTH")}) + "\n")
    return backend(environ, start_response)
run_simple("127.0.0.1", int(sys.argv[1]), app, threaded=True)
''', str(port), str(range_log)]
        with (root / "s3.log").open("wb") as log:
            server = subprocess.Popen(
                server_command,
                stdout=log, stderr=log,
            )
            try:
                connection = dict(endpoint_url=endpoint, region_name="us-east-1",
                                  aws_access_key_id="graphit-test-root",
                                  aws_secret_access_key="graphit-test-root-secret")
                s3 = boto3.client("s3", **connection)
                ready_by = time.monotonic() + 20
                while True:
                    if server.poll() is not None:
                        raise RuntimeError("isolated S3 test server exited during startup")
                    try:
                        s3.list_buckets()
                        break
                    except Exception:
                        if time.monotonic() >= ready_by:
                            raise RuntimeError("isolated S3 test server did not become ready") from None
                        time.sleep(.1)
                bucket = "graphit-records-test"
                s3.create_bucket(Bucket=bucket)
                issued = boto3.client("sts", **connection).assume_role(
                    RoleArn="arn:aws:iam::123456789012:role/graphit-records-test",
                    RoleSessionName="graphit-records-product-test", DurationSeconds=3600,
                )["Credentials"]
                fixture = root / "fixture.json"
                with open(fixture, "x", opener=lambda path, flags: os.open(path, flags, 0o600)) as output:
                    json.dump(dict(endpoint=endpoint, bucket=bucket, region="us-east-1",
                                   access_key_id=issued["AccessKeyId"],
                                   secret_access_key=issued["SecretAccessKey"],
                                   session_token=issued["SessionToken"],
                                   expires_at=issued["Expiration"].isoformat()), output)
                environment = os.environ.copy()
                # These compound, unoptimized Rust fixtures nest real model,
                # record and SDK/S3 futures. Their polling stack exceeds the
                # libtest default; this changes test threads only, not daemon
                # runtime settings, deadlines or assertions.
                environment.setdefault("RUST_MIN_STACK", str(8 * 1024 * 1024))
                environment["GRAPHIT_TEST_S3_FIXTURE"] = str(fixture)
                status = subprocess.run(
                    [str(binary), args.filter, "--ignored",
                     "--nocapture", "--test-threads=1"], env=environment,
                ).returncode
                if status == 0 and args.require_parquet_ranges:
                    calls = [json.loads(line) for line in range_log.read_text().splitlines()]
                    reads = [(i, call) for i, call in enumerate(calls) if call["method"] == "GET"]
                    full = [(i, call) for i, call in reads if not call["range"]]
                    # This exact fixture deliberately fetches one original object,
                    # appends one byte and PUTs it back to test changed-object rejection.
                    # It is not a query read. Identify that operation by its complete
                    # sequence; no other full GET is permitted.
                    corruption_test = "source_server::remote_native::tests::official_ladybug_progressive_publication_and_remote_queries_s3"
                    if args.filter == corruption_test:
                        if len(full) != 1:
                            raise RuntimeError(f"expected only the corruption fixture's full GET; observed {len(full)}")
                        i, fetched = full[0]
                        replacement = calls[i + 1] if i + 1 < len(calls) else {}
                        uploaded = next((call for call in reversed(calls[:i])
                                         if call["method"] == "PUT" and call["path"] == fetched["path"]), {})
                        if ("/immutable/version-one/graph/" not in fetched["path"]
                                or replacement.get("method") != "PUT"
                                or replacement.get("path") != fetched["path"]
                                or int(replacement.get("size") or 0) != int(uploaded.get("size") or 0) + 1):
                            raise RuntimeError("full GET was not the explicit one-byte corruption fixture")
                    elif full:
                        raise RuntimeError("Icebug queries must not perform full Parquet GETs")
                    ranges = [call for _, call in reads if call["range"]]
                    if not ranges or any(not call["range"].startswith("bytes=") for call in ranges):
                        raise RuntimeError("Icebug must read Parquet with HTTP ranges, without full GET")
                    print(f"Verified {len(ranges)} Parquet query HTTP Range requests; "
                          f"{len(full)} separately verified corruption-fixture full GET")
                return status
            finally:
                server.terminate()
                try:
                    server.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    server.kill()
                    server.wait()


if __name__ == "__main__":
    sys.exit(main())
