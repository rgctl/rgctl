# Common module - base utilities

class common {
  # Clean code - no security issues

  package { 'vim':
    ensure => installed,
  }

  package { 'curl':
    ensure => installed,
  }

  package { 'wget':
    ensure => installed,
  }

  file { '/etc/motd':
    ensure  => file,
    owner   => 'root',
    group   => 'root',
    mode    => '0644',
    content => "Welcome to UAT Test Server\n",
  }

  # NTP configuration
  service { 'chronyd':
    ensure => running,
    enable => true,
  }
}
