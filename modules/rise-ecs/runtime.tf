module "runtime" {
  source = "./modules/runtime"

  cluster = {
    name = local.cluster_name
    arn  = local.cluster_arn
  }
  discovery = {
    namespace_id   = local.namespace_id
    namespace_name = local.namespace_name
  }
  network = {
    control_plane = {
      subnet_ids         = local.private_subnet_ids
      security_group_ids = [aws_security_group.control_plane.id]
    }
    traefik = {
      subnet_ids         = local.private_subnet_ids
      security_group_ids = [aws_security_group.traefik.id]
    }
  }
  roles = {
    execution     = var.execution_role_arn
    control_plane = var.controller_role_arn
    traefik       = local.traefik_task_role_arn
  }
  logging = {
    group_name = aws_cloudwatch_log_group.this.name
    region     = local.region
  }
  control_plane = {
    name                   = "${local.name}-control-plane"
    discovery_name         = local.control_plane_discovery_name
    image                  = local.rise_image_ref
    cpu                    = var.rise_cpu
    memory                 = var.rise_memory
    desired_count          = var.rise_desired_count
    enable_execute_command = var.enable_execute_command
    labels                 = module.control_plane_env.docker_labels
  }
  environment        = local.rise_environment
  secret_environment = local.control_plane_secret_environment
  traefik = {
    name            = "${local.name}-traefik"
    discovery_name  = local.traefik_discovery_name
    image           = var.traefik_image
    refresh_seconds = var.traefik_refresh_seconds
    constraints     = var.traefik_constraints
  }
  acme = {
    enabled         = local.acme_enabled
    email           = var.acme_email
    file_system_id  = try(aws_efs_file_system.acme[0].id, null)
    access_point_id = try(aws_efs_access_point.acme[0].id, null)
  }
  load_balancer_targets = { for port, group in aws_lb_target_group.traefik : port => group.arn }
  cpu_architecture      = var.cpu_architecture
  tags                  = local.tags

  # Tasks need populated secrets, mounted storage, and attached target groups.
  depends_on = [
    aws_secretsmanager_secret_version.database_url,
    aws_secretsmanager_secret_version.jwt_signing_secret,
    aws_secretsmanager_secret_version.encryption_key,
    aws_secretsmanager_secret_version.oidc_client_secret,
    aws_lb_listener.http,
    aws_lb_listener.https,
    aws_lb_listener.alb_https,
    aws_efs_mount_target.acme,
  ]
}
