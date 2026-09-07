#!/usr/bin/env python3
"""Verify supported ECS state layouts using isolated, mocked Terraform upgrades.

Requires the baseline Git object and Terraform on PATH. The production module's
initialized provider lock and binaries are reused. No AWS
credentials or real infrastructure are used.
"""

import argparse
import json
import re
import shutil
import subprocess
import tarfile
import tempfile
from pathlib import Path

BASELINE = "348dd9e2ce2ff0eea15ab3cf44728bc073d46850"
PATHS = ["modules/rise-ecs", "tests/e2e/run", "dev/dex/config.yaml"]
MOCKS = r"""
mock_provider "aws" {
  mock_data "aws_caller_identity" { defaults = { account_id = "123456789012" } }
  mock_data "aws_region" { defaults = { id = "eu-central-1", region = "eu-central-1" } }
  mock_data "aws_partition" { defaults = { partition = "aws" } }
  mock_data "aws_availability_zones" { defaults = { names = ["eu-central-1a", "eu-central-1b"] } }
  mock_data "aws_iam_policy_document" { defaults = { json = "{}" } }
  mock_resource "aws_ecs_cluster" { defaults = { arn = "arn:aws:ecs:eu-central-1:123456789012:cluster/rise" } }
  mock_resource "aws_iam_role" { defaults = { arn = "arn:aws:iam::123456789012:role/rise-traefik" } }
  mock_resource "aws_secretsmanager_secret" { defaults = { arn = "arn:aws:secretsmanager:eu-central-1:123456789012:secret:rise/example-abc123" } }
  mock_resource "aws_efs_file_system" { defaults = { arn = "arn:aws:elasticfilesystem:eu-central-1:123456789012:file-system/fs-abc", id = "fs-abc" } }
  mock_resource "aws_efs_access_point" { defaults = { id = "fsap-abc" } }
  mock_resource "aws_service_discovery_service" { defaults = { arn = "arn:aws:servicediscovery:eu-central-1:123456789012:service/srv-abc" } }
  mock_resource "aws_ecs_task_definition" { defaults = { arn = "arn:aws:ecs:eu-central-1:123456789012:task-definition/rise:1" } }
  mock_resource "aws_lb" { defaults = { arn = "arn:aws:elasticloadbalancing:eu-central-1:123456789012:loadbalancer/net/rise/1234567890123456" } }
  mock_resource "aws_lb_target_group" { defaults = { arn = "arn:aws:elasticloadbalancing:eu-central-1:123456789012:targetgroup/rise/1234567890123456" } }
}
mock_provider "random" {}
"""


def run(args, cwd, **kwargs):
    return subprocess.run(args, cwd=cwd, check=True, timeout=240, **kwargs)


def prepare(repo, work, baseline):
    with (work / "baseline.tar").open("wb") as archive:
        run(["git", "archive", baseline, *PATHS], repo, stdout=archive)
    with tarfile.open(work / "baseline.tar") as archive:
        archive.extractall(work / "baseline", filter="data")
    for relative in PATHS:
        source, target = repo / relative, work / "current" / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        if source.is_dir():
            shutil.copytree(
                source,
                target,
                ignore=shutil.ignore_patterns(
                    ".terraform",
                    ".terraform.lock.hcl",
                    "terraform.tfstate*",
                    "*.tfvars",
                    "*.tfvars.json",
                ),
            )
        else:
            shutil.copy2(source, target)
    (work / "main.tf").write_text("""terraform {
  required_providers {
    aws = { source = "hashicorp/aws", version = ">= 6.50" }
    random = { source = "hashicorp/random", version = ">= 3.6" }
  }
}
""")
    module = repo / "modules/rise-ecs"
    shutil.copy2(module / ".terraform.lock.hcl", work / ".terraform.lock.hcl")
    shutil.copytree(
        module / ".terraform/providers", work / ".terraform/providers", symlinks=True
    )
    (work / "tests").mkdir()
    for case, relative in [("nlb", PATHS[0]), ("alb", PATHS[0]), ("e2e", PATHS[1])]:
        source = (work / "baseline" / relative / "tests/plan.tftest.hcl").read_text()
        variables = re.search(
            r"^variables \{.*?^\}", source, re.MULTILINE | re.DOTALL
        ).group()
        overrides = ""
        if case == "e2e":
            overrides = re.search(
                r"^override_data \{\n  target = data.terraform_remote_state.bootstrap.*?^\}",
                source,
                re.MULTILINE | re.DOTALL,
            ).group()
        if case == "alb":
            variables = re.sub(
                r'acme_email\s*= "ops@example.com"',
                '''acme_email = null
  edge_mode = "alb-acm"
  acm_certificate_arn = "arn:aws:acm:eu-central-1:123456789012:certificate/12345678-1234-1234-1234-123456789012"''',
                variables,
            )
        runs = ""
        for phase, command in [("baseline", "apply"), ("current", "plan")]:
            runs += f'''run "{phase}" {{
  command = {command}
  state_key = "{case}"
  module {{ source = "./{phase}/{relative}" }}
}}
'''
        (work / "tests" / f"{case}.tftest.hcl").write_text(
            MOCKS + overrides + "\n" + variables + "\n" + runs,
        )


def verify(log):
    checked = set()
    expected_moves = {
        f"module.runtime.{kind}.{name}": f"{kind}.{name}"
        for kind in [
            "aws_ecs_task_definition",
            "aws_ecs_service",
            "aws_service_discovery_service",
        ]
        for name in ["rise", "traefik"]
    }
    e2e_fields = {
        "module.runtime.aws_ecs_task_definition.rise": {"container_definitions"},
        "module.runtime.aws_ecs_task_definition.traefik": {"container_definitions"},
        "module.runtime.aws_ecs_service.rise": {"enable_execute_command"},
        "module.runtime.aws_ecs_service.traefik": {
            "deployment_minimum_healthy_percent",
            "deployment_maximum_percent",
            "propagate_tags",
        },
    }
    for line in log.read_text().splitlines():
        item = json.loads(line)
        if (
            item.get("type") == "diagnostic"
            and item["diagnostic"]["severity"] == "error"
        ):
            raise AssertionError(item["diagnostic"])
        if item.get("type") != "test_plan" or item.get("@testrun") != "current":
            continue
        case = Path(item["@testfile"]).name.split(".")[0]
        changes = item["test_plan"]["resource_changes"]
        moves = {
            c["address"]: c["previous_address"]
            for c in changes
            if "previous_address" in c
        }
        assert moves == expected_moves, (case, "incomplete state moves", moves)
        for change in changes:
            address, delta = change["address"], change["change"]
            if delta["actions"] == ["no-op"]:
                continue
            assert case == "e2e" and address in e2e_fields, (
                case,
                address,
                delta["actions"],
            )
            assert delta["actions"] == ["update"], (case, address, delta["actions"])
            before, after = delta["before"], delta["after"]
            fields = {
                key
                for key in before.keys() | after.keys()
                if before.get(key) != after.get(key)
            }
            assert fields <= e2e_fields[address], (case, address, fields)
        checked.add(case)
        print(
            f"{case}: six state moves; "
            + (
                "only expected E2E task/service updates"
                if case == "e2e"
                else "no resource actions"
            )
        )
    assert checked == {"nlb", "alb", "e2e"}, ("missing upgrade plans", checked)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline-ref", default=BASELINE)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    terraform = shutil.which("terraform")
    if not terraform:
        parser.error("terraform must be on PATH")
    with tempfile.TemporaryDirectory(prefix="rise-runtime-upgrade-") as directory:
        work = Path(directory)
        prepare(repo, work, args.baseline_ref)
        run(
            [terraform, "init", "-backend=false", "-input=false", "-no-color"],
            work,
            stdout=subprocess.DEVNULL,
        )
        log = work / "results.jsonl"
        with log.open("w") as output:
            result = subprocess.run(
                [terraform, "test", "-json", "-verbose"],
                cwd=work,
                stdout=output,
                stderr=subprocess.STDOUT,
                timeout=240,
                check=False,
            )
        verify(log)
        assert result.returncode == 0, "Terraform upgrade test failed"


if __name__ == "__main__":
    main()
