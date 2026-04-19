{{/*
Common labels for all Observable resources.
Context must contain .Chart and .Release.
*/}}
{{- define "observable-common.labels" -}}
helm.sh/chart: {{ .Chart.Name }}-{{ .Chart.Version | replace "+" "_" }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/part-of: observable
{{- end }}

{{/*
Selector labels for a specific service component.
Context must contain .name and .Release.
*/}}
{{- define "observable-common.selectorLabels" -}}
app.kubernetes.io/name: {{ .name }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
Full image reference from global.image values.
Context must contain .Values.global.image.{repository,tag}.
*/}}
{{- define "observable-common.image" -}}
{{ .Values.global.image.repository }}:{{ .Values.global.image.tag | default "latest" }}
{{- end }}
