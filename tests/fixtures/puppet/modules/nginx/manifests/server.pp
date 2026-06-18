class nginx::server ($listen_port = 443) {
  include nginx

  file { '/etc/nginx/sites-enabled/default':
    ensure => file,
    mode   => '0644',
  }
}

define nginx::vhost ($port) {
  file { "/etc/nginx/sites-available/${title}":
    ensure => file,
    mode   => '0644',
  }
}
