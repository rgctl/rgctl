# Network Terraform definitions for HCL correctness matrix (UTF-8: α β)

variable "region" {
  type    = string
  default = "eu-central-1"
}

module "chargeback_vpc" {
  source = "./modules/vpc"
  cidr   = "10.20.0.0/16"
}

output "vpc_endpoint" {
  value = module.chargeback_vpc.endpoint
}
