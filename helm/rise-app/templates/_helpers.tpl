{{- define "rise-app.name" -}}
{{- default .Chart.Name .Values.rise.argocd.applicationName | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "rise-app.labels" -}}
app.kubernetes.io/name: {{ include "rise-app.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
app.kubernetes.io/part-of: rise
{{ include "rise-app.riseLabels" . }}
{{- end -}}

{{- define "rise-app.riseLabels" -}}
rise.dev/project: {{ .Values.rise.project.name | quote }}
rise.dev/deployment-group: {{ .Values.rise.deployment.normalizedGroup | quote }}
rise.dev/deployment-id: {{ .Values.rise.deployment.id | quote }}
{{- end -}}

{{- define "rise-app.selectorLabels" -}}
app.kubernetes.io/name: {{ include "rise-app.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end -}}

{{- define "rise-app.serviceAccountName" -}}
{{- if .Values.serviceAccount.create -}}
{{- default (include "rise-app.name" .) .Values.serviceAccount.name -}}
{{- else -}}
{{- default "default" .Values.serviceAccount.name -}}
{{- end -}}
{{- end -}}
