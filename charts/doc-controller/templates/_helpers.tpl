{{- define "controller.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{- define "controller.fullname" -}}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- $name | trunc 63 | trimSuffix "-" }}
{{- end }}

{{- define "controller.labels" -}}
{{- include "controller.selectorLabels" . }}
app.kubernetes.io/name: {{ include "controller.name" . }}
app.kubernetes.io/version: {{ .Values.image.tag | default .Chart.AppVersion | quote }}
{{- end }}

{{- define "controller.selectorLabels" -}}
app: {{ include "controller.name" . }}
{{- end }}

