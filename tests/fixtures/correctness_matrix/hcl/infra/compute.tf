# Compute Terraform definitions for HCL correctness matrix (UTF-8: λ → Ω)

locals {
  common_tags = {
    service = "chargeback-ledger"
  }
}

resource "aws_instance" "reconcile_chargeback_ledger" {
  ami           = "ami-0123456789abcdef0"
  instance_type = var.region == "eu-central-1" ? "t3.medium" : "t3.small"
  tags          = merge(local.common_tags, { env = var.region })
}

resource "aws_instance" "archive_chargeback_audit" {
  ami           = "ami-0123456789abcdef0"
  instance_type = "t3.small"
}
