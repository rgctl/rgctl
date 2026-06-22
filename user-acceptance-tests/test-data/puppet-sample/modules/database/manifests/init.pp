# Database module - PostgreSQL setup

class database (
  String $db_user = 'postgres',
  String $db_password = undef,
  Integer $port = 5432,
) inherits database::base {
  include common

  package { 'postgresql':
    ensure => installed,
  }

  service { 'postgresql':
    ensure  => running,
    enable  => true,
    require => Package['postgresql'],
  }

  file { '/etc/postgresql/postgresql.conf':
    ensure  => file,
    owner   => 'postgres',
    group   => 'postgres',
    mode    => '0600',
    content => template('database/postgresql.conf.erb'),
    notify  => Service['postgresql'],
  }

  # Resource relationship - file requires package
  file { '/var/lib/postgresql/data':
    ensure  => directory,
    owner   => 'postgres',
    group   => 'postgres',
    mode    => '0700',
    require => Package['postgresql'],
  }

  # Use Hiera lookup (safe)
  $backup_enabled = lookup('database::backup_enabled', Boolean, 'first', false)

  if $backup_enabled {
    cron { 'pg_dump':
      command => '/usr/bin/pg_dump -U postgres mydb > /backup/mydb.sql',
      hour    => 2,
      minute  => 0,
    }
  }
}

# Base class (inherited)
class database::base {
  # Common database configuration
}
