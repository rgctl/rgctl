# Webserver module - main class
# SECURITY NOTE: This module contains intentional vulnerabilities for testing!

class webserver (
  Integer $port = 80,
  String $admin_email = 'admin@example.com',
  String $server_name = $facts['hostname'],
) {
  include common

  # CWE-78: Command injection - unquoted variable in exec
  exec { 'restart-webserver':
    command => "/usr/bin/systemctl restart httpd && echo $server_name",
    path    => '/usr/bin:/bin',
  }

  # CWE-798: Hardcoded secret in file content
  file { '/etc/webserver/config.conf':
    ensure  => file,
    owner   => 'root',
    group   => 'root',
    mode    => '0777',  # CWE-732: World-writable file
    content => "password=SuperSecret123!",
  }

  package { 'httpd':
    ensure => installed,
    notify => Service['httpd'],
  }

  service { 'httpd':
    ensure  => running,
    enable  => true,
    require => Package['httpd'],
  }

  # Use a Puppet fact
  if $facts['os']['family'] == 'RedHat' {
    package { 'httpd-tools':
      ensure => installed,
    }
  }

  # Variable assignment
  $document_root = '/var/www/html'
  $ssl_enabled = true

  notify { "Webserver configured on port ${port}": }
}
