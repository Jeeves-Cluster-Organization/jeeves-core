#!/usr/bin/env python3
"""
Centralized Deployment Module for 7-Agent Assistant.

Provides a unified interface for deploying services in different configurations:
- Development: All services with hot reload
- Testing: Services + test runner with test dependencies
- Production: Optimized services only
- Gateway-only: For frontend development

Usage:
    python scripts/deployment/deploy.py --profile test
    python scripts/deployment/deploy.py --profile prod --build
    python scripts/deployment/deploy.py --services gateway api --build
    python scripts/deployment/deploy.py --validate
    python scripts/deployment/deploy.py --test-gateway
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
import time
import socket
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import Dict, List, Optional, Set


class ServiceName(str, Enum):
    """Available services."""
    LLAMA_SERVER = "llama-server"
    POSTGRES = "postgres"
    API = "assistant"
    GATEWAY = "gateway"
    TEST = "test"


class DeploymentProfile(str, Enum):
    """Deployment profiles."""
    DEV = "dev"
    TEST = "test"
    PROD = "prod"
    GATEWAY_ONLY = "gateway-only"
    MINIMAL = "minimal"  # Just API + deps, no gateway


@dataclass
class ServiceConfig:
    """Configuration for a single service."""
    name: ServiceName
    depends_on: List[ServiceName] = field(default_factory=list)
    health_check_port: Optional[int] = None
    health_check_path: Optional[str] = None
    health_check_type: str = "http"  # http, tcp, postgres
    required_in_profiles: Set[DeploymentProfile] = field(default_factory=set)


# Service definitions with dependencies and health checks
SERVICES: Dict[ServiceName, ServiceConfig] = {
    ServiceName.POSTGRES: ServiceConfig(
        name=ServiceName.POSTGRES,
        depends_on=[],
        health_check_port=5432,
        health_check_type="postgres",
        required_in_profiles={DeploymentProfile.DEV, DeploymentProfile.TEST,
                             DeploymentProfile.PROD, DeploymentProfile.MINIMAL},
    ),
    ServiceName.LLAMA_SERVER: ServiceConfig(
        name=ServiceName.LLAMA_SERVER,
        depends_on=[],
        health_check_port=8080,
        health_check_path="/health",
        health_check_type="http",
        required_in_profiles={DeploymentProfile.DEV, DeploymentProfile.TEST,
                             DeploymentProfile.PROD, DeploymentProfile.MINIMAL},
    ),
    ServiceName.API: ServiceConfig(
        name=ServiceName.API,
        depends_on=[ServiceName.POSTGRES, ServiceName.LLAMA_SERVER],
        health_check_port=8000,  # API_PORT default
        health_check_type="tcp",  # gRPC uses TCP
        required_in_profiles={DeploymentProfile.DEV, DeploymentProfile.TEST,
                             DeploymentProfile.PROD, DeploymentProfile.GATEWAY_ONLY,
                             DeploymentProfile.MINIMAL},
    ),
    ServiceName.GATEWAY: ServiceConfig(
        name=ServiceName.GATEWAY,
        depends_on=[ServiceName.API],
        health_check_port=8001,  # GATEWAY_PORT default
        health_check_path="/health",
        health_check_type="http",
        required_in_profiles={DeploymentProfile.DEV, DeploymentProfile.TEST,
                             DeploymentProfile.PROD, DeploymentProfile.GATEWAY_ONLY},
    ),
    ServiceName.TEST: ServiceConfig(
        name=ServiceName.TEST,
        depends_on=[ServiceName.POSTGRES, ServiceName.LLAMA_SERVER],
        health_check_type="none",
        required_in_profiles={DeploymentProfile.TEST},
    ),
}


class DeploymentManager:
    """Manages Docker Compose deployments."""

    def __init__(self, project_root: Optional[Path] = None, verbose: bool = False):
        self.project_root = project_root or Path(__file__).parent.parent.parent
        self.verbose = verbose
        self.env = self._load_env()

    def _load_env(self) -> Dict[str, str]:
        """Load environment variables from .env file."""
        env = os.environ.copy()
        env_file = self.project_root / ".env"
        if env_file.exists():
            with open(env_file) as f:
                for line in f:
                    line = line.strip()
                    if line and not line.startswith("#") and "=" in line:
                        key, value = line.split("=", 1)
                        env[key] = value
        return env

    def _run_command(self, cmd: List[str], check: bool = True,
                     capture_output: bool = False) -> subprocess.CompletedProcess:
        """Run a shell command."""
        if self.verbose:
            print(f"  [CMD] {' '.join(cmd)}")
        return subprocess.run(
            cmd,
            cwd=self.project_root,
            env=self.env,
            check=check,
            capture_output=capture_output,
            text=True,
        )

    def _get_port(self, service: ServiceName) -> int:
        """Get the host port for a service."""
        port_map = {
            ServiceName.POSTGRES: int(self.env.get("POSTGRES_PORT", "5432")),
            ServiceName.LLAMA_SERVER: int(self.env.get("LLAMASERVER_PORT", "8080")),
            ServiceName.API: int(self.env.get("API_PORT", "8000")),
            ServiceName.GATEWAY: int(self.env.get("GATEWAY_PORT", "8001")),
        }
        return port_map.get(service, SERVICES[service].health_check_port or 0)

    def get_services_for_profile(self, profile: DeploymentProfile) -> List[ServiceName]:
        """Get list of services required for a profile."""
        return [
            svc.name for svc in SERVICES.values()
            if profile in svc.required_in_profiles
        ]

    def resolve_dependencies(self, services: List[ServiceName]) -> List[ServiceName]:
        """Resolve service dependencies and return ordered list."""
        resolved: List[ServiceName] = []
        pending = set(services)

        while pending:
            for svc in list(pending):
                config = SERVICES[svc]
                # Check if all dependencies are resolved
                if all(dep in resolved for dep in config.depends_on):
                    resolved.append(svc)
                    pending.remove(svc)
                else:
                    # Add missing dependencies
                    for dep in config.depends_on:
                        if dep not in resolved and dep not in pending:
                            pending.add(dep)

        return resolved

    def check_health(self, service: ServiceName, timeout: int = 5) -> bool:
        """Check if a service is healthy."""
        config = SERVICES[service]
        port = self._get_port(service)

        if config.health_check_type == "none":
            return True

        if config.health_check_type == "tcp":
            try:
                sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                sock.settimeout(timeout)
                sock.connect(("localhost", port))
                sock.close()
                return True
            except (socket.error, socket.timeout):
                return False

        if config.health_check_type == "http":
            import urllib.request
            try:
                url = f"http://localhost:{port}{config.health_check_path or ''}"
                urllib.request.urlopen(url, timeout=timeout)
                return True
            except Exception:
                return False

        if config.health_check_type == "postgres":
            result = self._run_command(
                ["docker", "compose", "exec", "-T", "postgres",
                 "pg_isready", "-U", self.env.get("POSTGRES_USER", "assistant")],
                check=False,
                capture_output=True,
            )
            return result.returncode == 0

        return False

    def build(self, services: Optional[List[ServiceName]] = None,
              no_cache: bool = False) -> bool:
        """Build Docker images for services."""
        print("\n==> Building Docker images...")
        cmd = ["docker", "compose", "build"]
        if no_cache:
            cmd.append("--no-cache")
        if services:
            cmd.extend([s.value for s in services])
        try:
            self._run_command(cmd)
            print("    Build completed successfully")
            return True
        except subprocess.CalledProcessError as e:
            print(f"    Build failed: {e}")
            return False

    def start(self, services: List[ServiceName], detach: bool = True) -> bool:
        """Start services."""
        ordered = self.resolve_dependencies(services)
        service_names = [s.value for s in ordered]

        print(f"\n==> Starting services: {', '.join(service_names)}")
        cmd = ["docker", "compose", "up"]
        if detach:
            cmd.append("-d")
        cmd.extend(service_names)

        try:
            self._run_command(cmd)
            return True
        except subprocess.CalledProcessError as e:
            print(f"    Start failed: {e}")
            return False

    def stop(self, services: Optional[List[ServiceName]] = None) -> bool:
        """Stop services."""
        print("\n==> Stopping services...")
        cmd = ["docker", "compose", "down"]
        if services:
            # For selective stop, use 'stop' instead of 'down'
            cmd = ["docker", "compose", "stop"] + [s.value for s in services]
        try:
            self._run_command(cmd)
            return True
        except subprocess.CalledProcessError:
            return False

    def wait_for_healthy(self, services: List[ServiceName],
                         timeout: int = 180, interval: int = 5) -> bool:
        """Wait for all services to become healthy."""
        print(f"\n==> Waiting for services to be healthy (timeout: {timeout}s)...")

        # Filter services that have health checks
        checkable = [s for s in services if SERVICES[s].health_check_type != "none"]

        start_time = time.time()
        while time.time() - start_time < timeout:
            statuses = {svc: self.check_health(svc) for svc in checkable}

            status_str = " ".join(
                f"{svc.value}={'OK' if ok else '...'}"
                for svc, ok in statuses.items()
            )
            elapsed = int(time.time() - start_time)
            print(f"    Health: {status_str} ({elapsed}/{timeout}s)")

            if all(statuses.values()):
                print("    All services healthy!")
                return True

            time.sleep(interval)

        print("    Timeout waiting for services")
        return False

    def run_tests(self, test_args: Optional[List[str]] = None,
                  gateway_tests: bool = False) -> bool:
        """Run tests in the test container."""
        print("\n==> Running tests...")

        cmd = ["docker", "compose", "run", "--rm", "test", "pytest"]

        if gateway_tests:
            cmd.extend(["tests/integration/test_gateway.py", "-v"])
        elif test_args:
            cmd.extend(test_args)
        else:
            cmd.extend(["-v", "--tb=short"])

        try:
            result = self._run_command(cmd, check=False)
            return result.returncode == 0
        except Exception as e:
            print(f"    Tests failed: {e}")
            return False

    def validate(self) -> bool:
        """Validate the deployment."""
        print("\n==> Validating deployment...")

        all_ok = True
        checks = [
            ("Docker running", lambda: self._run_command(
                ["docker", "info"], check=False, capture_output=True
            ).returncode == 0),
            (".env exists", lambda: (self.project_root / ".env").exists()),
            ("docker-compose.yml exists", lambda: (self.project_root / "docker-compose.yml").exists()),
        ]

        # Add service health checks for running services
        result = self._run_command(
            ["docker", "compose", "ps", "--format", "json"],
            check=False, capture_output=True
        )
        if result.returncode == 0 and result.stdout.strip():
            for svc in ServiceName:
                if SERVICES[svc].health_check_type != "none":
                    checks.append((
                        f"{svc.value} is healthy",
                        lambda s=svc: self.check_health(s)
                    ))

        for name, check_fn in checks:
            try:
                ok = check_fn()
                status = "OK" if ok else "FAIL"
                print(f"    [{status}] {name}")
                if not ok:
                    all_ok = False
            except Exception as e:
                print(f"    [FAIL] {name}: {e}")
                all_ok = False

        return all_ok

    def deploy(self, profile: DeploymentProfile,
               build: bool = False, wait: bool = True,
               no_cache: bool = False) -> bool:
        """Deploy services for a given profile."""
        print(f"\n{'='*60}")
        print(f"  Deploying profile: {profile.value}")
        print(f"{'='*60}")

        services = self.get_services_for_profile(profile)

        if build:
            if not self.build(services, no_cache=no_cache):
                return False

        if not self.start(services):
            return False

        if wait:
            if not self.wait_for_healthy(services):
                return False

        print(f"\n==> Deployment complete!")
        self._print_service_urls(services)
        return True

    def _print_service_urls(self, services: List[ServiceName]):
        """Print URLs for running services."""
        print("\nService URLs:")
        if ServiceName.GATEWAY in services:
            port = self._get_port(ServiceName.GATEWAY)
            print(f"  Gateway UI:    http://localhost:{port}/chat")
            print(f"  Gateway Docs:  http://localhost:{port}/docs")
        if ServiceName.API in services:
            port = self._get_port(ServiceName.API)
            print(f"  API (gRPC):    localhost:{port}")
        if ServiceName.LLAMA_SERVER in services:
            port = self._get_port(ServiceName.LLAMA_SERVER)
            print(f"  LLM Server:    http://localhost:{port}/health")


def main():
    parser = argparse.ArgumentParser(
        description="Deploy 7-Agent Assistant services",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Deploy for testing
  python deploy.py --profile test

  # Deploy for production with build
  python deploy.py --profile prod --build

  # Deploy specific services
  python deploy.py --services gateway api --build

  # Run gateway tests
  python deploy.py --test-gateway

  # Validate deployment
  python deploy.py --validate

  # Stop all services
  python deploy.py --stop
        """,
    )

    parser.add_argument(
        "--profile", "-p",
        type=str,
        choices=[p.value for p in DeploymentProfile],
        help="Deployment profile to use",
    )
    parser.add_argument(
        "--services", "-s",
        nargs="+",
        choices=[s.value for s in ServiceName],
        help="Specific services to deploy",
    )
    parser.add_argument(
        "--build", "-b",
        action="store_true",
        help="Build images before starting",
    )
    parser.add_argument(
        "--no-cache",
        action="store_true",
        help="Build without using cache (forces fresh build)",
    )
    parser.add_argument(
        "--no-wait",
        action="store_true",
        help="Don't wait for services to be healthy",
    )
    parser.add_argument(
        "--stop",
        action="store_true",
        help="Stop all services",
    )
    parser.add_argument(
        "--validate",
        action="store_true",
        help="Validate deployment",
    )
    parser.add_argument(
        "--test-gateway",
        action="store_true",
        help="Run gateway integration tests",
    )
    parser.add_argument(
        "--test",
        nargs="*",
        help="Run tests with optional pytest args",
    )
    parser.add_argument(
        "--verbose", "-v",
        action="store_true",
        help="Verbose output",
    )

    args = parser.parse_args()

    manager = DeploymentManager(verbose=args.verbose)

    if args.stop:
        sys.exit(0 if manager.stop() else 1)

    if args.validate:
        sys.exit(0 if manager.validate() else 1)

    if args.test_gateway:
        # Deploy test profile first
        if not manager.deploy(DeploymentProfile.TEST, build=args.build):
            sys.exit(1)
        sys.exit(0 if manager.run_tests(gateway_tests=True) else 1)

    if args.test is not None:
        # Deploy test profile first
        if not manager.deploy(DeploymentProfile.TEST, build=args.build):
            sys.exit(1)
        sys.exit(0 if manager.run_tests(test_args=args.test if args.test else None) else 1)

    if args.profile:
        profile = DeploymentProfile(args.profile)
        success = manager.deploy(profile, build=args.build, wait=not args.no_wait,
                                 no_cache=args.no_cache)
        sys.exit(0 if success else 1)

    if args.services:
        services = [ServiceName(s) for s in args.services]
        if args.build:
            manager.build(services, no_cache=args.no_cache)
        if manager.start(services):
            if not args.no_wait:
                manager.wait_for_healthy(services)
        sys.exit(0)

    parser.print_help()


if __name__ == "__main__":
    main()
