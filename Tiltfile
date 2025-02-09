# Usage default features:
# tilt up
#
# Usage with features:
# tilt up telemetry
config.define_string("features", args=True)
cfg = config.parse()
features = cfg.get('features', "")
print("compiling with features: {}".format(features))

IMG = 'kube-rs/controller'
local_resource('compile', 'just compile %s' % features)
docker_build(IMG, '.')
# NB: for the image to be pullable by kubernetes via k3d
k8s_yaml('yaml/crd.yaml')
k8s_yaml(helm('./charts/doc-controller', set=['image.repository=' + IMG]))
k8s_resource('doc-controller', port_forwards=8080)
