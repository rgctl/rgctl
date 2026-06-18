# nginx module — init manifest
class nginx ($port = 80) inherits nginx::base {
  include common

  if $facts['os']['family'] == 'debian' {
    package { 'nginx':
      ensure => installed,
      notify => Service['nginx'],
    }
  }

  service { 'nginx':
    ensure  => running,
    require => Package['nginx'],
  }

  file { '/etc/nginx/nginx.conf':
    ensure  => file,
    owner   => 'root',
    mode    => '0666',
    content => 'password=hardcodedsecret123',
  }

  exec { 'reload':
    command => "/bin/sh -c echo $hostname",
    path    => '/usr/bin:/bin',
  }

  $web_port = $port
}
