local_resource('compile', 'just compile')
docker_build('clux/controller', '.', dockerfile='Dockerfile')
k8s_yaml('yaml/deployment.yaml')
k8s_resource('foo-controller', port_forwards=8080)
