#
# Cookbook:: nginx
# Recipe:: default
#

include_recipe 'common::default'

package 'nginx' do
  action :install
end

service 'nginx' do
  action [:enable, :start]
  notifies :restart, 'service[nginx]'
end

template '/etc/nginx/nginx.conf' do
  source 'nginx.conf.erb'
  mode '0666'
  owner 'root'
end

execute 'reload nginx' do
  command "echo #{node['user_input']}"
end
